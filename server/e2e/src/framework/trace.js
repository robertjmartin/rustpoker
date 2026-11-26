// Opt-in wire trace for a single test.
//
// `e2eTest(..., { trace: true })` creates one of these and runs the scenario
// inside its AsyncLocalStorage context. Framework helpers call `currentTrace()`
// and record what actually crosses the wire. Nothing is recorded for tests that
// do not ask for it.
import { AsyncLocalStorage } from 'node:async_hooks';
import { closeSync, mkdirSync, openSync, writeSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const TRACE_DIR = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '../../traces',
);

const storage = new AsyncLocalStorage();

/** The trace for the running test, or null when tracing is off. */
export function currentTrace() {
  return storage.getStore() ?? null;
}

export class TestTrace {
  #fd;
  #started;
  #closed = false;
  #connections = 0;

  constructor(name) {
    mkdirSync(TRACE_DIR, { recursive: true });
    this.file = path.join(TRACE_DIR, `${slug(name)}.log`);
    this.#fd = openSync(this.file, 'w');
    this.#started = performance.now();
    this.record('test', '*', 'start', name);
  }

  /** Run `fn` with this trace visible to framework helpers. */
  run(fn) {
    return storage.run(this, fn);
  }

  /** Stable label for a client that has no player name. */
  nextConnectionLabel() {
    return `ws#${++this.#connections}`;
  }

  /** Append one event: channel, direction, optional label and payload. */
  record(channel, direction, label, payload) {
    if (this.#closed) return;
    const elapsed = `${(performance.now() - this.#started).toFixed(1)}ms`;
    const detail = payload === undefined ? '' : ` ${format(payload)}`;
    const line =
      `${elapsed.padStart(9)} ${channel.padEnd(6)} ${direction.padEnd(2)}` +
      `${label ? ` ${label}` : ''}${detail}`;
    writeSync(this.#fd, line + '\n');
  }

  close() {
    if (this.#closed) return;
    this.record('test', '*', 'end');
    this.#closed = true;
    closeSync(this.#fd);
  }
}

function format(payload) {
  return typeof payload === 'string' ? payload : JSON.stringify(payload);
}

function slug(name) {
  const value = name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 80);
  return value || 'test';
}
