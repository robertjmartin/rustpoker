// Owns the pokerserver child process: build check, launch, port discovery,
// deterministic ticks, and shutdown. Tests never spawn processes themselves.
import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { requestJson } from './http.js';
import { currentTrace } from './trace.js';

const FRAMEWORK_DIR = path.dirname(fileURLToPath(import.meta.url));
export const SERVER_DIR = path.resolve(FRAMEWORK_DIR, '../../..');
export const SERVER_BIN = path.join(SERVER_DIR, 'target', 'debug', 'pokerserver');

const DEFAULT_TICK_MS = 60_000;

/**
 * Start a clean server process.
 *
 * The port is always requested as 0 and discovered from the server's
 * "listening on" line, so any number of these can run at the same time.
 * The tick interval defaults to a minute because tests advance the table with
 * `server.tick()` (backed by the test-only POST /test/tick route).
 */
export async function startServer({
  tickMs = DEFAULT_TICK_MS,
  testHooks = true,
  env = {},
  startupTimeoutMs = 10_000,
} = {}) {
  const trace = currentTrace();
  if (!existsSync(SERVER_BIN)) {
    throw new Error(
      `pokerserver binary not found at ${SERVER_BIN}\n` +
        'Build it with: cd server/e2e && npm run build',
    );
  }

  const child = spawn(SERVER_BIN, [], {
    cwd: SERVER_DIR,
    env: {
      ...process.env,
      POKER_PORT: '0',
      POKER_TICK_MS: String(tickMs),
      POKER_TEST_HOOKS: testHooks ? '1' : '0',
      ...env,
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  trace?.record('server', 'start', '', `pid ${child.pid} ${SERVER_BIN}`);

  // Keep the tail of the server log. Failure messages include it, which is
  // usually enough to see a panic without re-running by hand.
  const output = [];
  const record = (chunk) => {
    for (const line of chunk.toString().split('\n')) {
      if (!line.trim()) continue;
      output.push(line);
      trace?.record('server', 'log', '', line);
      if (output.length > 300) output.shift();
    }
  };
  child.stdout.on('data', record);
  child.stderr.on('data', record);

  let spawnError = null;
  child.on('error', (error) => {
    spawnError = error;
  });

  const port = await new Promise((resolve, reject) => {
    let buffer = '';
    const timer = setTimeout(() => {
      finish(new Error(
        `pokerserver did not report a port within ${startupTimeoutMs}ms\n` +
          `--- server output ---\n${output.join('\n')}`,
      ));
    }, startupTimeoutMs);

    const scan = (chunk) => {
      buffer += chunk.toString();
      const match = /listening on \S*:(\d+)/.exec(buffer);
      if (match) finish(null, Number(match[1]));
    };
    const onExit = (code, signal) => {
      finish(new Error(
        `pokerserver exited before listening (code ${code}, signal ${signal})\n` +
          `--- server output ---\n${output.join('\n')}`,
      ));
    };
    const onError = (error) => finish(error);
    const finish = (error, value) => {
      clearTimeout(timer);
      child.stdout.off('data', scan);
      child.off('exit', onExit);
      child.off('error', onError);
      if (error) reject(error);
      else resolve(value);
    };

    child.stdout.on('data', scan);
    child.on('exit', onExit);
    child.on('error', onError);

    // The process may have failed before the listeners above were attached.
    if (spawnError) {
      finish(spawnError);
    } else if (child.exitCode !== null) {
      onExit(child.exitCode, child.signalCode);
    }
  });

  const closed = new Promise((resolve) => child.once('close', resolve));
  trace?.record('server', 'listening', '', `http://127.0.0.1:${port}`);

  return {
    port,
    host: '127.0.0.1',
    httpUrl: `http://127.0.0.1:${port}`,
    wsUrl: `ws://127.0.0.1:${port}`,
    child,
    get output() {
      return output.slice();
    },
    /** Run one table tick (deal / state-machine step) immediately. */
    async tick() {
      const result = await requestJson(`http://127.0.0.1:${port}`, 'POST', '/test/tick');
      if (result.status !== 200) {
        throw new Error(`POST /test/tick failed: HTTP ${result.status} (${result.text})`);
      }
      return result;
    },
    async stop() {
      trace?.record('server', 'stop', '', `pid ${child.pid}`);
      child.kill('SIGTERM');
      const timedOut = await Promise.race([
        closed.then(() => false),
        delay(2_000).then(() => true),
      ]);
      if (timedOut) {
        child.kill('SIGKILL');
        await closed;
      }
    },
  };
}

/** Start a server and register its shutdown with a node:test context. */
export async function launchServer(t, options = {}) {
  const server = await startServer(options);
  t.after(() => server.stop());
  return server;
}
