// WebSocket test client. It separates the two server message shapes:
//   * responses: { id, response: "success" | "error", ... } -> correlated by id
//   * pushes:    { event: "sit" | "deal" | ... }            -> queued for waits
import { setTimeout as delay } from 'node:timers/promises';

/** The server ignores the table id for now, but it must be a valid UUID. */
export const TABLE_ID = '00000000-0000-0000-0000-000000000000';

const DEFAULT_REQUEST_TIMEOUT_MS = 2_000;
const DEFAULT_EVENT_TIMEOUT_MS = 2_000;

export class TestClient {
  #server;
  #ws;
  #nextId = 0;
  #pending = new Map();
  #events = [];
  #responses = [];
  #malformed = [];
  #cursor = 0;
  #closed = false;

  constructor(server, ws) {
    this.#server = server;
    this.#ws = ws;
    ws.addEventListener('message', (event) => this.#handleMessage(event.data));
    ws.addEventListener('close', () => this.#rejectPending(new Error('websocket closed')));
  }

  static async connect(server, { timeoutMs = 5_000 } = {}) {
    const url = `${server.wsUrl}/ws`;
    const ws = new WebSocket(url);
    await new Promise((resolve, reject) => {
      const timer = setTimeout(
        () => reject(new Error(`websocket did not open within ${timeoutMs}ms: ${url}`)),
        timeoutMs,
      );
      ws.addEventListener('open', () => {
        clearTimeout(timer);
        resolve();
      }, { once: true });
      ws.addEventListener('error', () => {
        clearTimeout(timer);
        reject(new Error(`websocket failed to connect: ${url}`));
      }, { once: true });
    });
    return new TestClient(server, ws);
  }

  /** Every push received so far, in arrival order. */
  get events() {
    return this.#events.slice();
  }

  /** Every response received so far, including unsolicited/late ones. */
  get responses() {
    return this.#responses.slice();
  }

  /** Text frames the server sent that were not valid JSON (should stay empty). */
  get malformedFrames() {
    return this.#malformed.slice();
  }

  get isOpen() {
    return this.#ws.readyState === WebSocket.OPEN;
  }

  /** Send a request and resolve with the response that echoes its id. */
  request(command, fields = {}, { timeoutMs = DEFAULT_REQUEST_TIMEOUT_MS } = {}) {
    if (!this.isOpen) {
      throw new Error(`cannot send "${command}": websocket is not open`);
    }
    const id = ++this.#nextId;
    const payload = { id, command, ...fields };
    const response = new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.#pending.delete(id);
        reject(new Error(
          `no response to "${command}" (id ${id}) within ${timeoutMs}ms\n` +
            `sent: ${JSON.stringify(payload)}\n` +
            `recent pushes: ${JSON.stringify(this.#events.slice(-5))}\n` +
            `server output:\n${this.#server.output.slice(-20).join('\n')}`,
        ));
      }, timeoutMs);
      this.#pending.set(id, { resolve, reject, timer });
    });
    this.#ws.send(JSON.stringify(payload));
    return response;
  }

  /** Send a raw text frame, for malformed-input tests. */
  sendRaw(text) {
    if (!this.isOpen) {
      throw new Error('cannot send raw frame: websocket is not open');
    }
    this.#ws.send(text);
  }

  /**
   * Resolve with the next push matching `eventName` (and optional `where`
   * predicate). The internal cursor means repeated waits walk the stream
   * forward instead of re-matching old events.
   */
  async waitForEvent(eventName, { where = null, timeoutMs = DEFAULT_EVENT_TIMEOUT_MS } = {}) {
    const deadline = Date.now() + timeoutMs;
    for (;;) {
      for (let i = this.#cursor; i < this.#events.length; i++) {
        const event = this.#events[i];
        if (event.event === eventName && (!where || where(event))) {
          this.#cursor = i + 1;
          return event;
        }
      }
      if (Date.now() >= deadline) {
        throw new Error(
          `timed out after ${timeoutMs}ms waiting for "${eventName}"` +
            `${where ? ' matching predicate' : ''}\n` +
            `received: ${JSON.stringify(this.#events)}`,
        );
      }
      await delay(5);
    }
  }

  /** Poll until `predicate(events)` is true; useful for "N events arrived". */
  async waitFor(predicate, { description = 'condition', timeoutMs = DEFAULT_EVENT_TIMEOUT_MS } = {}) {
    const deadline = Date.now() + timeoutMs;
    for (;;) {
      if (predicate(this.#events)) return this.#events;
      if (Date.now() >= deadline) {
        throw new Error(
          `timed out after ${timeoutMs}ms waiting for ${description}\n` +
            `received: ${JSON.stringify(this.#events)}`,
        );
      }
      await delay(5);
    }
  }

  /** Assert the server stays silent for `withinMs` (e.g. unparseable frames). */
  async expectNoResponse({ withinMs = 300 } = {}) {
    const before = this.#responses.length;
    await delay(withinMs);
    if (this.#responses.length !== before) {
      throw new Error(
        `expected no response, got ${JSON.stringify(this.#responses.slice(before))}`,
      );
    }
  }

  /** Assert no event of this name has ever arrived on this connection. */
  async expectNoEvent(eventName, { withinMs = 300 } = {}) {
    await delay(withinMs);
    const found = this.#events.find((event) => event.event === eventName);
    if (found) {
      throw new Error(
        `expected no "${eventName}" event, received: ${JSON.stringify(found)}`,
      );
    }
  }

  setAuthentication(userId) {
    return this.request('setAuthentication', { userId });
  }

  getAuthentication() {
    return this.request('getAuthentication');
  }

  joinTable(tableId = TABLE_ID) {
    return this.request('joinTable', { tableId });
  }

  leaveTable() {
    return this.request('leaveTable');
  }

  tableAction(action, tableId = TABLE_ID) {
    return this.request('table', { tableId, action });
  }

  sit(bankAmount, tableId = TABLE_ID) {
    return this.tableAction({ action: 'sit', bankAmount }, tableId);
  }

  stand(tableId = TABLE_ID) {
    return this.tableAction({ action: 'stand' }, tableId);
  }

  bet(betAmount, tableId = TABLE_ID) {
    return this.tableAction({ action: 'bet', betAmount }, tableId);
  }

  fold(tableId = TABLE_ID) {
    return this.tableAction({ action: 'fold' }, tableId);
  }

  async close() {
    if (this.#closed) return;
    this.#closed = true;
    try {
      this.#ws.close();
    } catch {
      // Already closed.
    }
    await Promise.race([
      new Promise((resolve) => {
        if (this.#ws.readyState === WebSocket.CLOSED) resolve();
        else this.#ws.addEventListener('close', resolve, { once: true });
      }),
      delay(500),
    ]);
    this.#rejectPending(new Error('client closed'));
  }

  #handleMessage(raw) {
    let message;
    try {
      message = JSON.parse(raw);
    } catch {
      this.#malformed.push(String(raw));
      return;
    }

    // A response echoes the request id; pushes have no id.
    if (typeof message?.id === 'number' && 'response' in message) {
      this.#responses.push(message);
      const pending = this.#pending.get(message.id);
      if (pending) {
        this.#pending.delete(message.id);
        clearTimeout(pending.timer);
        pending.resolve(message);
      }
      return;
    }

    this.#events.push(message);
  }

  #rejectPending(error) {
    for (const pending of this.#pending.values()) {
      clearTimeout(pending.timer);
      pending.reject(error);
    }
    this.#pending.clear();
  }
}
