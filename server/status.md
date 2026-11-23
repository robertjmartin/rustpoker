# Server status and next steps

Last updated: 2026-11-22

This file tracks what the poker server does today and what is left to build. The
wire protocol reference is `protocol.md`; `design.md` holds the original design
notes and is partly out of date.

## What works today

Players can register over HTTP, connect over WebSocket, authenticate, join the
one table, sit with a stack, and play a hand from deal to showdown:

1. `Idle` opens a hand once at least two players are seated: the button moves to
   the next occupied seat, every dealt-in seat gets two hole cards, a `deal`
   event goes out, every dealt-in player pays a 2-chip ante, and the next two
   seats post a 10 small blind and a 20 big blind. A `turn` event names the
   first seat to act: left of the big blind, or the button when heads-up (where
   the button is the small blind).
2. Players bet, check or fold in turn order. The big blind sets the opening
   price, so preflop always has something to call. A `bet` is clamped to the
   stack, a `check` is only legal with nothing to call, and a `fold` takes the
   player out of the hand.
3. Once everyone who still has chips has acted and matched, the next street
   opens: `communityCards` carries 3 cards for the flop, then 1 for the turn and
   1 for the river, and action restarts left of the button.
4. The river round ends in `handEnd`. If one player remains they take the pot
   without a showdown; otherwise the best five of seven cards wins or splits the
   pot.
5. A later timer tick opens the next hand, with the button on the following
   seat.
6. Each turn arms a 30 s clock. On expiry the table checks when the actor owes
   nothing, folds otherwise. A closed WebSocket stands that player up; heads-up
   the hand ends on the next tick, but with three or more players a disconnect
   on turn freezes the hand (see Known gaps).

Chips live in a table pot and are conserved (unit-tested). A client that joins
or reconnects receives the table in a private `snapshot` and can ask for its
hand again with `getHoleCards`. The Solid client in `../client` is served by the
same process at `/` from a generated single-file HTML build
(`../scripts/build-client.sh`); `/test` remains as a protocol scratchpad.

## Implemented in this pass

### Core rules (`pokercore`)

- `table.rs` is a real state machine: `Idle → PreFlop → Flop → Turn → River`,
  with dealer rotation, turn order, per-street contributions, `acted` flags,
  all-in skipping and a table pot.
- Forced bets: a per-player ante (2) and small/big blinds (10/20), with the
  blinds clamped to a short stack and the nominal big blind still setting
  `current_bet`. Preflop starts left of the big blind; postflop starts left of
  the button. `Table::with_stakes` overrides the defaults.
- `snapshot` now carries `smallBlind`, `bigBlind` and `ante` so a joining client
  can show the stakes before any forced-bet event arrives.
- `hand.rs` (new): best-five-of-seven evaluator. Categories 8 (straight flush)
  down to 0 (high card), kickers in poker order, wheel (A-2-3-4-5) handled.
- Seats track `folded` and `acted`. `leave` mid-hand folds the player but keeps
  their chips in the pot; sitting down mid-hand waits for the next hand.
- Legality is enforced in the core: out-of-turn actions, checking into a bet and
  betting with no chips are rejected and emit no event.

### Events and protocol

- `GameEvent` gained `Check`, `Turn`, `CommunityCards` and `HandEnd` on top of
  `Sit`, `Leave`, `Deal`, `Bet` and `Fold`; forced bets added `Ante` and `Blind`
  (`kind: "small" | "big"`). All of them are serialised to JSON and broadcast
  to every joined connection.
- Hole cards are private: the core buffers `(SeatId, HoleCardEvent)` separately
  from the public events, the worker inverts `seat_map`, and only the owner
  receives the `holeCards` frame.
- `joinTable` is answered with a private `snapshot` frame: per-seat occupancy,
  stacks and bets, the board, pot, actor, dealer, street and the player's own
  cards. Late joiners and reconnects no longer start from an empty table.
- A showdown emits `showdown` with the cards of every player still in the pot,
  before `handEnd`. Folded hands stay private, and the client renders the
  revealed cards on their seats.
- `getHoleCards` lets a client ask again after joining or reconnecting. The ack
  is `success`; the private frame follows.
- `TableAction` gained `check`. The worker routes `check`, `fold`, `stand` and
  `leaveTable` into the core instead of dropping them.
- The 1 s tick broadcasts events produced by `Table::process()`. `DummyEvent`
  and its non-JSON `dummy event` frame are gone.
- `Table::process()` runs a turn clock: every new decision arms 30 one-second
  ticks, and an expired turn auto-checks (nothing to call) or auto-folds.
- When a socket task ends it sends `TableCommands::Disconnect`; the worker drops
  the connection and stands the seat up with `leave`, which folds a live hand
  and frees the seat on the next deal. Idle sockets are pinged every 10 s and
  dropped after 25 s without any frame, so a browser that vanishes silently is
  stood up too instead of leaving a live-looking seat behind. Standing up a seat
  that is on turn hits the turn-passing bug under Known gaps.
- The embedded client is served with `Cache-Control: no-store`, so a browser
  cannot run an older build of the single-file client against a newer server.

### Fixes from the initial review

- `bet` clamps with `min`, so a 50 bet from a 500 stack no longer takes 500.
- `player_count` ignores seats flagged `left`, so a departed player no longer
  counts toward the two-player start condition.
- Seat lookups use `get_mut`: a stale or out-of-range seat index returns `None`
  instead of panicking the table worker.
- Fan-out drops connections whose channel send fails.
- Event fan-out and the private hole-card path share one encoder (`send_private`).

## Layout

| Path | Role |
|---|---|
| `pokercore/src/deck.rs` | Cards, deck, shuffle, card wire shape |
| `pokercore/src/hand.rs` | Five-of-seven hand evaluator |
| `pokercore/src/table.rs` | Table state machine, pot, event buffer |
| `pokercore/src/game_event.rs` | Public and private event types |
| `pokercore/src/snapshot.rs` | Per-viewer `TableSnapshot` / `SeatSnapshot` wire shapes |
| `pokerserver/src/main.rs` | axum routes, WebSocket tasks, table worker, fan-out |
| `pokerserver/src/client_command.rs` | Inbound wire types, `TableAction` |
| `pokerserver/src/client_responses.rs` | Outbound response envelope |
| `pokerserver/src/table_command.rs` | Connection → worker commands |
| `pokerserver/static/test.html` | Browser test client on `/test` |
| `pokerserver/static/client.html` | Generated single-file Solid client, served on `/` |

## Protocol at a glance

Commands (`{"id": n, "command": ...}`): `setAuthentication`, `getAuthentication`,
`getHoleCards`, `joinTable`, `leaveTable`, `table` (whose `action` is `sit`,
`stand`, `bet`, `check` or `fold`).

Public events, broadcast to every joined client:

```json
{ "event": "sit",  "seat": 3 }
{ "event": "deal", "seatsDealt": [3, 4] }
{ "event": "ante", "seat": 3, "amount": 2 }
{ "event": "blind", "seat": 3, "amount": 10, "kind": "small" }
{ "event": "blind", "seat": 4, "amount": 20, "kind": "big" }
{ "event": "turn", "seat": 3 }
{ "event": "bet",  "seat": 3, "amount": 50 }
{ "event": "check", "seat": 3 }
{ "event": "fold", "seat": 3 }
{ "event": "communityCards", "cards": [ { "suit": "spades", "rank": "ace" } ] }
{ "event": "showdown", "hands": [ { "seat": 3, "cards": [ { "suit": "spades", "rank": "ace" } ] } ] }
{ "event": "handEnd", "winners": [3], "amount": 250, "showdown": true }
{ "event": "leave", "seat": 3 }
```

Per-connection frames, sent only to the socket that receives them:

```json
{ "event": "holeCards", "seat": 3, "cards": [ { "suit": "spades", "rank": "ace" }, { "suit": "hearts", "rank": "ten" } ] }
```

`joinTable` is answered with that connection's `snapshot`. `yourSeat` and
`yourCards` are filled only for the player who asked:

```json
{ "event": "snapshot", "seats": [ ... ], "board": [], "pot": 36, "currentBet": 20,
  "smallBlind": 10, "bigBlind": 20, "ante": 2, "acting": 0, "dealer": 0,
  "street": "preFlop", "yourSeat": 0, "yourCards": [ ... ] }
```

## Known gaps

- **Fixed stakes.** The table always uses 10/20 blinds and a 2-chip ante;
  there is no per-table configuration over the wire and no blind increases.
- **No side pots.** A short all-in can win the whole pot rather than only the
  part it covered. A big blind clamped to a short stack still sets the nominal
  price, so the uncalled part of a call is not returned.
- **A disconnect on turn freezes a three-handed or larger hand.** `leave` folds
  the departed seat but never passes the turn on. Heads-up the hand ends on the
  next tick; with three or more players the clock's auto-action cannot fold an
  already-folded seat, so `advance_turn` never runs, the hand parks on the empty
  seat forever and no new hand can start. Reproduced with three seated players
  (verified: `leave` only, `acting` stuck on the empty seat for over 30 s).
- **A disconnect forfeits the seat.** Any socket close, or a socket that stops
  answering pings for 25 s, stands the player up; they come back as a spectator
  and must sit down again with a new stack. A brief network drop therefore costs
  the seat.
- **Hands are paced by the 1 s tick.** The next hand opens up to 1 s after the
  previous one ends, and all-in run-outs advance one street per tick unless
  nobody can act.
- **Actions are fire-and-forget.** An illegal action is acknowledged with
  `success` and silently ignored; there is no per-action error or result.
- **Single table.** `tableId` is parsed and ignored.
- **No bankroll enforcement.** `sit` accepts any `bankAmount`, and the registry
  balance is never debited or credited.
- **`test.html` lags the protocol.** It has no check button and does not render
  the board, turn or hole cards.
- **In-memory only.** Restarting the server wipes users, balances and tables.
- **Code hygiene.** `#![allow(unused)]` is still on in both crates, the old
  fn-pointer `User` struct in `pokercore` is dead, and `getAuthentication`
  replies under the `setAuthentication` command name.
- **No committed integration test.** The multi-client verification was run ad
  hoc over raw sockets and the served client and is not in the repository.

## Next steps

Ordered by value to a playable game:

1. **Fix the disconnect freeze.** When the seat on turn leaves, run the leave
   through `after_action`/`advance_turn` (or have `process` skip a turn with no
   live actor) so the rest of the table can finish the hand. Add a core test for
   a three-handed mid-hand leave.
2. **Side pots.** Track each player's total contribution for the hand and split
   the pot into main/side layers at showdown; the evaluator already returns a
   comparable score per seat.
3. **Action results.** Return `error` (or a result payload) for out-of-turn,
   wrong-amount and not-seated actions instead of a bare `success`, likely with
   a request/response channel from the worker.
4. **Bankroll escrow.** Debit `User.balance` on `sit` (rejecting more than the
   user has), credit it on `stand`/`leave`, and pay winnings back into the
   registry rather than only into the seat.
5. **Configurable stakes.** Send blinds and ante with `joinTable` (or a table
   config route) so different tables can run different limits.
6. **Client test page.** Add the check button and render board cards, the turn
   marker, hole cards and the pot from the event stream.
7. **Committed integration test.** Turn the raw-socket script into a repo test
   (or a small Rust test with a websocket client) that plays a full hand, so the
   end-to-end path is covered by `cargo test`.
8. **Multi-table routing.** Keep a `HashMap<TableId, worker>` and route
   `joinTable`/`table` by the ID they already send.
9. **Cleanup.** Remove `#![allow(unused)]`, delete the dead `User` struct, fix
   the `getAuthentication` response name, and update `design.md` or fold it into
   `../README.md`.

## Verification

```bash
cd server
cargo test --workspace     # 35 unit tests: rules, events, evaluator, turn clock, blinds/antes
cargo build -p pokerserver
./target/debug/pokerserver # listens on 0.0.0.0:3000
```

`/test` covers the HTTP endpoints, the `sit`/`stand`/`bet`/`fold` actions and a
raw-message box for manual protocol checks. It has no check button and renders
no board, so it is a protocol scratchpad rather than a playable client.

The most recent end-to-end checks were run ad hoc and are not in the repository:

- two independent windows of the served Solid client (driven headlessly through
  its built bundle) register, sit, receive hole cards, act, and see each other's
  bets and the pot move;
- a raw client that completes the handshake, sits and then stops reading is
  stood up by the 25 s keepalive and its seat is freed;
- forced bets: the `ante`/`blind` events, starting pot and first actor;
- the three-handed disconnect freeze listed under Known gaps.
