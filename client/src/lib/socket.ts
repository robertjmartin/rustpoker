import type { ClientCommand, GameEvent, ServerResponse } from "./protocol";

export type SocketStatus = "connecting" | "open" | "closed";

/** Absolute server URL when VITE_SERVER_URL is set, otherwise same origin. */
export function socketUrl(): string {
  const base = import.meta.env.VITE_SERVER_URL as string | undefined;
  if (base) return `${base.replace(/^http/, "ws")}/ws`;
  const scheme = location.protocol === "https:" ? "wss:" : "ws:";
  return `${scheme}//${location.host}/ws`;
}

interface Pending {
  resolve: (response: ServerResponse) => void;
  reject: (error: Error) => void;
  timer: number;
}

const REQUEST_TIMEOUT_MS = 8000;
const MAX_RECONNECT_DELAY_MS = 8000;

/**
 * One WebSocket connection to `/ws`.
 *
 * Requests are correlated with responses by the numeric `id` the server echoes;
 * frames without an `id` are game events and are handed to `onEvent`. A dropped
 * connection is retried with backoff until `stop()` is called.
 */
export class PokerSocket {
  onStatus: (status: SocketStatus) => void = () => {};
  onEvent: (event: GameEvent) => void = () => {};
  onMalformed: (frame: string) => void = () => {};

  private socket: WebSocket | null = null;
  private pending = new Map<number, Pending>();
  private seq = 0;
  private attempt = 0;
  private stopped = true;
  private reconnectTimer: number | null = null;

  constructor(private readonly url: string) {}

  get isOpen(): boolean {
    return this.socket?.readyState === WebSocket.OPEN;
  }

  start(): void {
    if (!this.stopped) return;
    this.stopped = false;
    this.open();
  }

  stop(): void {
    this.stopped = true;
    if (this.reconnectTimer !== null) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.failPending("disconnected");
    const socket = this.socket;
    this.socket = null;
    socket?.close();
    this.onStatus("closed");
  }

  /** Send one command and resolve with its response envelope. */
  request(command: ClientCommand): Promise<ServerResponse> {
    const socket = this.socket;
    if (!socket || socket.readyState !== WebSocket.OPEN) {
      return Promise.reject(new Error("not connected"));
    }

    const id = ++this.seq;
    return new Promise<ServerResponse>((resolve, reject) => {
      const timer = window.setTimeout(() => {
        this.pending.delete(id);
        reject(new Error("the server did not answer"));
      }, REQUEST_TIMEOUT_MS);
      this.pending.set(id, { resolve, reject, timer });
      socket.send(JSON.stringify({ id, ...command }));
    });
  }

  private open(): void {
    this.onStatus("connecting");

    let socket: WebSocket;
    try {
      socket = new WebSocket(this.url);
    } catch {
      this.onStatus("closed");
      this.scheduleReconnect();
      return;
    }
    this.socket = socket;

    socket.onopen = () => {
      this.attempt = 0;
      this.onStatus("open");
    };

    socket.onmessage = (message) => this.receive(String(message.data));

    socket.onerror = () => {
      // A close event always follows; reconnect is handled there.
    };

    socket.onclose = () => {
      if (this.socket === socket) this.socket = null;
      this.failPending("connection closed");
      this.onStatus("closed");
      if (!this.stopped) this.scheduleReconnect();
    };
  }

  private receive(frame: string): void {
    let message: unknown;
    try {
      message = JSON.parse(frame);
    } catch {
      this.onMalformed(frame);
      return;
    }
    if (typeof message !== "object" || message === null) return;

    if ("id" in message) {
      const response = message as ServerResponse;
      const pending = this.pending.get(response.id);
      if (!pending) return;
      this.pending.delete(response.id);
      clearTimeout(pending.timer);
      pending.resolve(response);
      return;
    }

    if ("event" in message) this.onEvent(message as GameEvent);
  }

  private scheduleReconnect(): void {
    if (this.reconnectTimer !== null) return;
    const delay = Math.min(500 * 2 ** this.attempt, MAX_RECONNECT_DELAY_MS);
    this.attempt += 1;
    this.reconnectTimer = window.setTimeout(() => {
      this.reconnectTimer = null;
      if (!this.stopped) this.open();
    }, delay);
  }

  private failPending(reason: string): void {
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timer);
      pending.reject(new Error(reason));
    }
    this.pending.clear();
  }
}
