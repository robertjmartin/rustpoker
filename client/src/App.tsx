import { Show, createMemo, createSignal, onCleanup, onMount } from "solid-js";

import { ActionBar } from "./components/ActionBar";
import { EventLog, type LogEntry } from "./components/EventLog";
import { Lobby } from "./components/Lobby";
import { TableFelt } from "./components/TableFelt";
import { fetchUser, registerUser } from "./lib/api";
import { createGame } from "./lib/game";
import { SEAT_COUNT, TABLE_ID, type TableAction, type User } from "./lib/protocol";
import { PokerSocket, socketUrl, type SocketStatus } from "./lib/socket";

const STORAGE_KEY = "rustpoker.user";

function loadStoredUser(): User | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<User>;
    if (typeof parsed.id !== "string" || typeof parsed.name !== "string") return null;
    return { id: parsed.id, name: parsed.name, balance: Number(parsed.balance ?? 0) };
  } catch {
    return null;
  }
}

export default function App() {
  const [user, setUserRaw] = createSignal<User | null>(loadStoredUser());
  const [status, setStatus] = createSignal<SocketStatus>("closed");
  const [joined, setJoined] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  const [notice, setNotice] = createSignal("");
  const [entries, setEntries] = createSignal<LogEntry[]>([]);

  const game = createGame();
  const socket = new PokerSocket(socketUrl());

  let entryId = 0;
  const log = (kind: LogEntry["kind"], text: string) => {
    const time = new Date().toLocaleTimeString([], { hour12: false });
    setEntries((list) => [...list.slice(-199), { id: ++entryId, kind, text, time }]);
  };

  const setUser = (next: User | null) => {
    setUserRaw(next);
    if (next) localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
    else localStorage.removeItem(STORAGE_KEY);
  };

  socket.onEvent = (event) => {
    const line = game.apply(event);
    if (line) log("event", line);
  };

  socket.onStatus = (next) => {
    setStatus(next);
    if (next === "open") void authenticate();
    else if (next === "closed") setJoined(false);
  };

  async function authenticate() {
    const current = user();
    if (!current) return;
    try {
      const response = await socket.request({ command: "setAuthentication", userId: current.id });
      if (response.response === "error") {
        setNotice("The server does not know this player any more. Create a new one.");
        log("error", "setAuthentication failed: unknown user");
        setUser(null);
        return;
      }
      await joinTable();
    } catch (error) {
      log("error", (error as Error).message);
    }
  }

  async function joinTable() {
    try {
      const response = await socket.request({ command: "joinTable", tableId: TABLE_ID });
      if (response.response === "error") {
        setNotice(`Could not join the table: ${response.error}`);
        return;
      }
      setJoined(true);
      log("system", "Subscribed to table events");
      void socket.request({ command: "getHoleCards" }).catch(() => {});
    } catch (error) {
      setNotice((error as Error).message);
    }
  }

  async function handleRegister(name: string) {
    setBusy(true);
    setNotice("");
    try {
      const created = await registerUser(name);
      setUser(created);
      log("system", `Registered ${created.name} with ${created.balance} chips`);
      socket.start();
    } catch (error) {
      setNotice((error as Error).message);
    } finally {
      setBusy(false);
    }
  }

  async function handleResume(userId: string) {
    setBusy(true);
    setNotice("");
    try {
      const found = await fetchUser(userId);
      if (!found) {
        setNotice("No player with that id on the server.");
        return;
      }
      setUser(found);
      log("system", `Resumed as ${found.name}`);
      socket.start();
    } catch (error) {
      setNotice((error as Error).message);
    } finally {
      setBusy(false);
    }
  }

  function signOut() {
    void socket.request({ command: "leaveTable" }).catch(() => {});
    socket.stop();
    setJoined(false);
    setNotice("");
    game.reset();
    setUser(null);
    log("system", "Signed out");
  }

  async function sendAction(action: TableAction, description: string): Promise<boolean> {
    setNotice("");
    try {
      const response = await socket.request({ command: "table", tableId: TABLE_ID, action });
      if (response.response === "error") {
        setNotice(`Action rejected: ${response.error}`);
        log("error", `${description} — rejected (${response.error})`);
        return false;
      }
      log("mine", description);
      return true;
    } catch (error) {
      setNotice((error as Error).message);
      log("error", `${description} — ${(error as Error).message}`);
      return false;
    }
  }

  async function sit(amount: number) {
    game.noteSitRequested(amount);
    // If no sit event arrives the table was full or the seat was refused.
    window.setTimeout(() => game.cancelPendingSit(), 2000);
    const accepted = await sendAction({ action: "sit", bankAmount: amount }, `Sitting down with ${amount}`);
    if (!accepted) game.cancelPendingSit();
  }

  const myBet = () => {
    const seat = game.state.mySeat;
    return seat === null ? 0 : (game.state.seats[seat]?.bet ?? 0);
  };

  const streetMax = createMemo(() =>
    game.state.seats.reduce((max, seat) => (seat.occupied ? Math.max(max, seat.bet) : max), 0),
  );

  const toCall = createMemo(() => Math.max(0, streetMax() - myBet()));
  const isMyTurn = createMemo(() => game.state.acting !== null && game.state.acting === game.state.mySeat);
  const seatedCount = createMemo(() => game.state.seats.filter((seat) => seat.occupied).length);

  const street = createMemo(() => {
    if (game.state.result) return "Hand over";
    if (!game.state.inHand) return seatedCount() >= 2 ? "Next hand starting" : "Waiting for players";
    switch (game.state.board.length) {
      case 0:
        return "Pre-flop";
      case 3:
        return "Flop";
      case 4:
        return "Turn";
      default:
        return "River";
    }
  });

  const statusLabel = () =>
    status() === "open" ? "Connected" : status() === "connecting" ? "Connecting…" : user() ? "Reconnecting…" : "Offline";

  onMount(() => {
    const stored = user();
    if (!stored) {
      log("system", "Welcome. Create a player to join the table.");
      return;
    }
    void (async () => {
      try {
        const found = await fetchUser(stored.id);
        if (!found) {
          setNotice("Saved player not found on the server (it keeps players in memory). Create a new one.");
          log("system", "Saved player is gone");
          setUser(null);
          return;
        }
        setUser(found);
        log("system", `Welcome back, ${found.name}`);
        socket.start();
      } catch (error) {
        setNotice(`Could not reach the server: ${(error as Error).message}`);
        log("error", "Server unreachable — is pokerserver running on port 3000?");
      }
    })();
  });

  onCleanup(() => socket.stop());

  return (
    <div class="app">
      <header class="topbar">
        <div class="brand">
          <span class="pip">♠</span> rustpoker
        </div>
        <div class="topbar-right">
          <span class={`conn ${status()}`}>
            <i class="dot" />
            {statusLabel()}
          </span>
          <Show when={user()}>
            {(current) => (
              <>
                <span class="who">{current().name}</span>
                <span class="chips">{current().balance} chips</span>
                <button class="ghost" type="button" onClick={signOut}>
                  Sign out
                </button>
              </>
            )}
          </Show>
        </div>
      </header>

      <main>
        <Show
          when={user()}
          fallback={
            <Lobby busy={busy()} notice={notice()} onRegister={handleRegister} onResume={handleResume} />
          }
        >
          <section class="table-shell">
            <div class="table-head">
              <span class="street">{street()}</span>
              <Show when={game.state.bigBlind > 0}>
                <span class="blinds">
                  Blinds {game.state.smallBlind}/{game.state.bigBlind}
                  <Show when={game.state.ante > 0}> · ante {game.state.ante}</Show>
                </span>
              </Show>
              <span class="spacer" />
              <span class="players">
                {seatedCount()} / {SEAT_COUNT} seated
              </span>
            </div>

            <TableFelt state={game.state} />

            <Show
              when={joined()}
              fallback={
                <div class="join-bar">
                  <span>
                    {status() === "open" ? "Not subscribed to the table." : "Connecting to the server…"}
                  </span>
                  <button class="primary" type="button" disabled={status() !== "open"} onClick={() => void joinTable()}>
                    Join table
                  </button>
                </div>
              }
            >
              <ActionBar
                seated={game.state.mySeat !== null}
                seat={game.state.mySeat}
                stack={game.state.myStack}
                turn={isMyTurn()}
                acting={game.state.acting}
                inHand={game.state.inHand}
                toCall={toCall()}
                onSit={(amount) => void sit(amount)}
                onStand={() => void sendAction({ action: "stand" }, "You stood up")}
                onBet={(amount) => void sendAction({ action: "bet", betAmount: amount }, `You bet ${amount}`)}
                onCheck={() => void sendAction({ action: "check" }, "You check")}
                onFold={() => void sendAction({ action: "fold" }, "You fold")}
              />
            </Show>

            <p class="footnote">
              Your stack is restored from the table snapshot on join and kept in step with your antes,
              blinds, bets and payouts; live events carry actions, cards and the pot.
            </p>

            <Show when={notice()}>
              <p class="notice">{notice()}</p>
            </Show>
          </section>

          <EventLog entries={entries()} onClear={() => setEntries([])} />
        </Show>
      </main>
    </div>
  );
}
