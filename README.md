# Rust Poker Server

## 1. Overview
 
`rustpoker` is a Rust poker server. Clients connect over HTTP (account registration, lookups) and WebSocket (authentication, joining a table, playing). The game logic is kept in a separate library crate so it has no dependency on networking.
 
```
rustpoker/
└── server/                     (cargo workspace, edition 2024)
    ├── pokercore/              library: game rules and state
    │   └── src/{lib,deck,table,game_event}.rs
    └── pokerserver/            binary: axum HTTP + WebSocket server
        └── src/{main,client_command,client_responses,table_command}.rs
```
 
| Crate | Type | Key dependencies | Responsibility |
|---|---|---|---|
| `pokercore` | lib | `rand` | Cards, deck, table state machine, game events. No I/O, no async. |
| `pokerserver` | bin | `axum` (ws), `tokio`, `serde`, `serde_json`, `uuid`, `pokercore` | Network protocol, user registry, hosting the table, fan-out of events to clients. |
 
## 2. Architecture
 
```
 ┌──────────┐  HTTP   ┌─────────────────────────────────────────┐
 │  Client  │────────▶│ axum Router (port 3000)                 │
 │          │  WS     │  /ping /register /user/{id} /usercount  │
 │          │◀───────▶│  /ws  → handle_socket (1 task / client) │
 └──────────┘         └───────┬─────────────────────▲───────────┘
                              │ TableCommand        │ GameEvent
                              │ (mpsc, cap 100)     │ (mpsc per client, cap 100)
                              ▼                     │
                      ┌───────────────────────────────────┐
                      │ worker task (owns pokercore Table)│
                      │  - handles TableCommands          │
                      │  - 1s timer → table.process()     │
                      │  - broadcasts GameEvents          │
                      └───────────────────────────────────┘
```
 
Design points:
 
- **Single-owner table.** The `Table` is owned by one `worker` task. It is never shared behind a lock. All mutations arrive as `TableCommand` messages on an `mpsc` channel, so game state needs no synchronisation.
- **Per-connection tasks.** Each WebSocket gets its own task (`handle_socket`) and its own `mpsc::channel<GameEvent>`. The sender half is handed to the worker on `Join`, and the worker pushes events through it.
- **Tick-driven game loop.** The worker also runs a one-second `interval`, which calls `Table::process()` to run the turn clock (30 s per actor), advance the state machine and deal the next hand, then sends events to all connected clients.
- **Shared user registry.** `AppState.users` is an `Arc<Mutex<HashMap<UserId, User>>>`, in-memory only 
- **Layering.** `pokercore` knows nothing about users, sockets or JSON. `pokerserver` translates wire-level commands into `TableCommand`s and `GameEvent`s into wire messages.

## 3. External interfaces (pokerserver)
 
The server listens on `0.0.0.0:3000`.
 
### 3.1 HTTP
 
| Method | Path | Request body | Response |
|---|---|---|---|
| GET | `/ping` | – | `{"status": "ok"}` |
| POST | `/register` | `{"name": "<string>"}` | `201 Created`, `User` JSON (see below) |
| GET | `/user/{id}` | – | `200` with `User` JSON, or `404` if unknown |
| GET | `/usercount` | – | `{"count": <number>}` |
| GET | `/ws` | – | Upgrades to WebSocket (see 3.2) |
 
`User` JSON:
 
```json
{ "id": "<uuid>", "name": "alice", "balance": 1000 }
```
 
New users start with a balance of `1000`. The `id` is a v4 UUID and acts as the user's identity for the WebSocket `setAuthentication` command.
 
### 3.2 WebSocket protocol
 
All messages are JSON text frames. Field names are `camelCase`.
 
#### Request envelope
 
Every client request carries a client-chosen numeric `id` (u32) used to correlate the response, plus a `command` discriminator. Command fields are flattened into the same object.
 
```json
{ "id": 1, "command": "<commandName>", ...fields }
```
 
| `command` | Extra fields | Notes |
|---|---|---|
| `setAuthentication` | `userId` (uuid) | Succeeds if `userId` exists in the registry; binds the connection to that user. |
| `getAuthentication` | – | Returns the connection's current user. |
| `getHoleCards` | – | Requires authentication. Asks the worker to resend this player's current hand as a private `holeCards` message. |
| `joinTable` | `tableId` (uuid) | Requires authentication. Registers the connection to receive table events. |
| `leaveTable` | – | Currently acknowledged but not acted on. |
| `table` | `tableId` (uuid), `action` (object) | Requires authentication. Sends a table action (below). |
 
Examples:
 
```json
{ "id": 1, "command": "setAuthentication", "userId": "6f1c…" }
{ "id": 2, "command": "joinTable", "tableId": "…" }
{ "id": 3, "command": "table", "tableId": "…", "action": { "action": "sit", "bankAmount": 500 } }
```
 
#### Table actions
 
The nested `action` object is itself tagged by an `action` field:
 
| `action` | Extra fields | Intended meaning |
|---|---|---|
| `sit` | `bankAmount` (u32) | Take a seat with the given stack. |
| `stand` | – | Leave the seat. |
| `bet` | `betAmount` (u32) | Place a bet. |
| `check` | – | Pass the action when there is nothing to call. |
| `fold` | – | Fold the hand. |
 
#### Response envelope
 
Every request that parses produces one response echoing the request `id`.
 
Success:
 
```json
{ "id": 1, "response": "success", "command": "setAuthentication", "userId": "6f1c…" }
{ "id": 2, "response": "success", "command": "joinTable" }
{ "id": 3, "response": "success", "command": "table" }
```
 
Error:
 
```json
{ "id": 2, "response": "error", "error": "notAuthenticated" }
```
 
| `error` | When |
|---|---|
| `notAuthenticated` | `joinTable` or `table` before a successful `setAuthentication`. |
| `unknown` | Unknown `userId` on `setAuthentication`, or `getAuthentication` while unauthenticated. |
 
Notes on current behaviour:
 
- A `success` response to `table` only means the command was queued for the worker. It does not report the outcome of the action (for example, whether `sit` found a free seat).
- An action that is illegal right now (out of turn, checking into a bet, betting with no chips) is acknowledged and then ignored; the event stream shows only legal actions.
- A message that fails to parse is logged server-side and **no response is sent**.
- `getAuthentication` currently reports success using the `setAuthentication` command name in the response.
- If a player does not act within 30 seconds the table acts for them: a `check` when nothing is owed, otherwise a `fold`. These arrive as ordinary `check`/`fold` events; there is no separate timeout event.
- Closing the WebSocket stands the player up. The worker removes the connection, folds/leaves the seat and broadcasts `leave`; a reconnecting client is a spectator until it sits down again.
#### Server-pushed events
 
The worker pushes table events to every client that has joined a table. Each `GameEvent` is serialised once to JSON and the same text frame is sent to all joined connections, so a client that joined before sitting still sees other players' actions. A client that joins mid-hand gets the current table in the private snapshot described below instead of a replay of past events.

```json
{ "event": "sit",   "seat": 3 }
{ "event": "deal",  "seatsDealt": [3, 4] }
{ "event": "ante",  "seat": 3, "amount": 2 }
{ "event": "blind", "seat": 3, "amount": 10, "kind": "small" }
{ "event": "blind", "seat": 4, "amount": 20, "kind": "big" }
{ "event": "bet",   "seat": 3, "amount": 50 }
{ "event": "check", "seat": 3 }
{ "event": "fold",  "seat": 3 }
{ "event": "turn",  "seat": 3 }
{ "event": "communityCards", "cards": [ { "suit": "spades", "rank": "ace" }, … ] }
{ "event": "showdown", "hands": [ { "seat": 3, "cards": [ { "suit": "spades", "rank": "ace" }, { "suit": "hearts", "rank": "ten" } ] } ] }
{ "event": "handEnd", "winners": [3], "amount": 250, "showdown": true }
{ "event": "leave", "seat": 3 }
```

`Seat` is the zero-based seat index and `amount` is the number of chips actually taken, which may be less than the requested bet if it was clamped to the player's stack. Before the first voluntary action of a hand the table posts an `ante` for every player dealt in and a `small` and `big` blind; a blind is a live bet (it counts toward the player's round contribution), while the ante is dead money that only adds to the pot.

At a showdown a `showdown` frame with the cards of every player still in the pot is sent just before `handEnd`, so clients can show what won. Hands folded earlier are never published.

Hole cards are deliberately not part of this fan-out; they are the private messages below.
 
#### Private messages
 
Hole cards and the join snapshot are the only per-player messages so far. After a deal the worker sends each seated player their own hand, and nobody else:
 
```json
{ "event": "holeCards", "seat": 3, "cards": [ { "suit": "spades", "rank": "ace" }, { "suit": "hearts", "rank": "ten" } ] }
````seat` is the recipient's own seat, and cards use lowercase suit and rank names. A connection is sent its hand inside the snapshot on join (below); after that it can ask again with `{ "command": "getHoleCards" }`, which is acknowledged with `success` and followed by the same private `holeCards` message (nothing is sent if the user is not seated or has not been dealt in).

#### Snapshot on join

Immediately after `joinTable` is accepted the worker sends that connection a
private snapshot of the table. It carries the information a client would
otherwise have to rebuild from the event stream, so a late join or a reconnect
is coherent straight away:

```json
{
  "event": "snapshot",
  "seats": [
    { "seat": 0, "occupied": true, "stack": 450, "bet": 50, "folded": false },
    { "seat": 1, "occupied": true, "stack": 500, "bet": 0, "folded": true }
  ],
  "board": [ { "suit": "spades", "rank": "ace" }, … ],
  "pot": 100,
  "currentBet": 50,
  "smallBlind": 10,
  "bigBlind": 20,
  "ante": 2,
  "acting": 0,
  "dealer": 1,
  "street": "flop",
  "yourSeat": 0,
  "yourCards": [ { "suit": "hearts", "rank": "king" }, { "suit": "clubs", "rank": "nine" } ]
}
```

`seats` has one entry per seat, occupied or not, with the chips behind
(`stack`), the current betting round's contribution (`bet`) and whether the
player has folded. A player who stood up mid-hand is already reported as not
occupied. `acting` and `dealer` are `null` when no hand is running, and `street`
is `idle`, `preFlop`, `flop`, `turn` or `river`. `smallBlind`, `bigBlind` and
`ante` are the table's fixed stakes, so a client can show them before the first
forced-bet event arrives. `yourCards` is filled only for
the seat that owns the connection (`yourSeat`), so the frame is private and can
be sent without leaking another player's hand. There is no replay of past
events: the snapshot is the starting point, and every later change arrives as a
normal event.

## 4. Internal interfaces
 
### 4.1 Server-internal messages (`table_command.rs`)
 
The contract between the connection tasks and the table worker:
 
```rust
pub struct TableCommand {
    pub user_id: UserId,
    pub cmd: TableCommands,
}
 
pub enum TableCommands {
    Join(JoinTableCommand),        // carries the connection's event sender
    Leave,
    Disconnect(DisconnectCommand), // socket ended: remove connection, stand the seat up
    HoleCards,                     // resend this user's hand, privately
    Action(ActionTableCommand),  // carries a TableAction
}

pub struct JoinTableCommand { pub tx: mpsc::Sender<GameEvent> }
pub struct DisconnectCommand { pub tx: mpsc::Sender<String> }
pub struct ActionTableCommand { pub action: TableAction }
```
 
Channels:
 
| Channel | Direction | Payload | Capacity |
|---|---|---|---|
| `AppState.table_tx` → worker `rx` | connections → worker | `TableCommand` | 100 |
| per-connection `tx` → `handle_socket` `rx` | worker → connection | `String` (one JSON-encoded event) | 100 |
 
Worker-local state: `connections: HashMap<UserId, Sender<String>>` and `seat_map: HashMap<UserId, u8>` (user to seat index, populated on a successful `sit`, removed on `stand`, `leaveTable` and `Disconnect`). On `Join` the worker sends that connection a private `snapshot` frame before it resumes the normal loop. After every command and every tick the worker drains the table's events with `Table::drain_events()` and fans them out, then drains `Table::drain_hole_cards()` and sends each hand only to the user occupying that seat (`seat_map` inverted); a failed send drops that connection from the map. When a socket task ends it sends `Disconnect` itself, so a player who closes the tab is stood up even if no broadcast would have noticed the dead connection. The command carries the dying connection's sender; if the user already reconnected on a newer socket, the worker ignores the late disconnect and keeps that connection.
 
The worker exits when all `table_tx` senders are dropped (`rx.recv()` returns `None`).
 
### 4.2 Wire types (`client_command.rs`, `client_responses.rs`)
 
| Type | Role |
|---|---|
| `UserId(Uuid)` | Identity, serialisable, used as a map key. |
| `TableId(Uuid)` | Table identifier (deserialize only). |
| `ClientMessage { id, command }` | Request envelope. |
| `ClientCommand` | `SetAuthentication`, `GetAuthentication`, `GetHoleCards`, `JoinTable`, `LeaveTable`, `Table`. |
| `TableAction` | `Sit`, `Stand`, `Bet`, `Fold`. Reused directly inside `TableCommand`, so the wire type is also the internal type. |
| `ClientMessageResponse { id, response }` | Response envelope. |
| `ClientResponse` | `Success(ClientCommandResponse)` or `Error(ClientCommandError)`. |
| `ClientAuthResponse` | Defined but not yet used. |
 
Request handling is centralised in `process_client_command(state, &mut auth_user_id, &tx, cmd) -> ClientResponse`. Authentication state is per connection and held as `Option<UserId>` in `handle_socket`.
 
## 5. Game core interface (pokercore)
 
### 5.1 Public API`lib.rs` exports:

```rustpub use deck::Card;
pub use game_event::{GameEvent, SitEvent, LeaveEvent, DealEvent, BetEvent, FoldEvent, CheckEvent, TurnEvent, CommunityCardsEvent, ShowdownEvent, ShowdownHand, HandEndEvent, HoleCardEvent};
pub use hand::HandRank;
pub use table::{Table, SeatId, BetAmount};
pub use snapshot::{TableSnapshot, SeatSnapshot, StreetName};
```
 
`Deck` and the internals of the `table` module stay private. `Card` is exported because it appears in `HoleCardEvent` and in the server's hole-card message.
 
### 5.2 `Table`
 
```rust
pub struct Table { /* state, users, seats, deck, dealer_pos, current_pos, cards[5] */ }
 
impl Table {
    pub fn new(size: u8) -> Table;                         // size = number of seats, default 10/20 blinds, 2 ante
    pub fn with_stakes(size: u8, small_blind: u32, big_blind: u32, ante: u32) -> Table;
    pub fn player_count(&self) -> u8;                      // occupied seats
    pub fn sit(&mut self, bank: u32) -> Option<u8>;        // seat index, None if full
    pub fn leave(&mut self, seat: u8) -> Option<u32>;      // marks seat as left, returns bank; None if empty
    pub fn bet(&mut self, seat: u8, bet_amount: u32) -> Option<u32>; // None unless this seat is to act
    pub fn check(&mut self, seat: u8) -> bool;             // pass when there is nothing to call
    pub fn fold(&mut self, seat: u8) -> bool;              // fold the hand
    pub fn process(&mut self);                             // once a second: turn clock, new hands, run-outs
    pub fn drain_events(&mut self) -> Vec<GameEvent>;      // events produced since the last call
    pub fn drain_hole_cards(&mut self) -> Vec<(SeatId, HoleCardEvent)>;  // private per-seat hands
    pub fn hole_cards(&self, seat: u8) -> Option<HoleCardEvent>;          // current hand at a seat
    pub fn snapshot(&self, viewer: Option<u8>) -> TableSnapshot;          // whole table, private to one viewer
}
```
 
Internal model:
 
- `Seat { bank, bet, left, folded, acted, cards: [Option<Card>; 2] }`
- `Table.small_blind`, `Table.big_blind`, `Table.ante`: fixed forced bets, set by `Table::new` (10/20/2) or `Table::with_stakes`.
- `TableState`: `Idle`, `PreFlop`, `Flop`, `Turn`, `River`
- `Table.cards: [Option<Card>; 5]` for community cards (not yet dealt)
- `Table.users: Vec<Option<User>>` where `User` holds `game_event_callback` and `hole_card_callback` function pointers. This is an early sketch of the event delivery design and is not used by the server, which uses channels instead.
Lifecycle implemented so far:
 
1. `Idle`: remove seats flagged `left`; if more than one player is seated, move the dealer button to the next occupied seat, shuffle and deal two hole cards to every seat. Every dealt-in player then pays the ante, the two seats left of the button post the small and big blinds, and `PreFlop` opens. Action starts left of the big blind; heads-up the button posts the small blind and acts first.
2. `PreFlop` → `Flop` → `Turn` → `River`: one betting round each. The big blind sets the opening `current_bet`, so preflop always has a price. A round ends once every player who still has chips has acted and matched `current_bet`; all-in players are skipped, and a short bet simply leaves the player due to act again. The flop adds 3 community cards, the turn and river 1 each, and action restarts left of the button (the big blind heads-up).
3. After the river round the remaining hands are compared with `hand::best_hand` and the pot is split between the winners; if everyone else folds, the last player takes the pot without a showdown. Either way a `HandEnd` is emitted and the table returns to `Idle`.
4. Each new decision arms a 30-tick (one-second) clock in `push_turn`. `process()` counts it down; at zero `auto_act` checks when the actor owes nothing and folds otherwise, then the hand continues or ends normally.
 
Stakes are fixed per table: `Table::new` uses a 10 small blind, a 20 big blind and a 2-chip ante per player; `Table::with_stakes` sets them explicitly. The ante is posted by every dealt-in player and goes straight to the pot; the blinds are live bets and count toward the preflop call. A blind is clamped to a short stack, but `current_bet` stays at the nominal big blind, so the uncalled part is not returned (there are no side pots).

Simplifications: no side pots, so a short all-in stack can win the whole pot; no minimum raise; `leave` mid-hand folds the player but their chips stay in the pot until the hand ends. An idle player is checked or folded automatically after 30 s, and a socket that closes stands its player up.
 
### 5.3 Deck and cards (`deck.rs`, internal)
 
```rust
pub enum Rank { Two = 2, … Ace = 14 }     // Ord
pub enum Suit { Hearts, Diamonds, Spades, Clubs }
pub struct Card { suit, rank, deck_id }   // Card::get_value() -> (Rank, Suit)
pub struct Deck { cards, deck_id }        // new(), shuffle(), count(), deal() -> Option<Card>
```
 
Each `Deck` gets a unique `DeckId` from a global atomic counter, and every `Card` records which deck it came from. Shuffling uses `rand::thread_rng`. `Card` serialises as `{ "suit": "spades", "rank": "ace" }` with lowercase names; the internal `DeckId` is not exposed on the wire.
 
### 5.4 Events (`game_event.rs`)
 
```rust
pub enum GameEvent {
    Sit(SitEvent),                       // { seat }
    Leave(LeaveEvent),                   // { seat }
    Deal(DealEvent),                     // { seats_dealt }
    Ante(AnteEvent),                     // { seat, amount }
    Blind(BlindEvent),                   // { seat, amount, kind: "small" | "big" }
    Bet(BetEvent),                       // { seat, amount }
    Check(CheckEvent),                   // { seat }
    Fold(FoldEvent),                     // { seat }
    Turn(TurnEvent),                     // { seat }
    CommunityCards(CommunityCardsEvent), // { cards }
    Showdown(ShowdownEvent),             // { hands: [{ seat, cards }] }
    HandEnd(HandEndEvent),               // { winners, amount, showdown }
}
 
pub struct HoleCardEvent { cards: [Card; 2] }   // private to the player, not yet part of GameEvent
```
 
Events are the output of the core: the table emits them and the server fans them out to clients. They are buffered on the table and handed over by `drain_events()`, which keeps the core free of I/O. `sit`, `leave`, `bet`, `check` and `fold` emit their own event, as do the forced `ante` and `blind` posts at the start of a hand; opening a street emits `CommunityCards`, every change of actor emits `Turn`, a showdown emits `Showdown` with the cards of every player still in the pot (folded hands are never revealed), and a finished hand emits `HandEnd` with the winners and the pot they took. `HoleCardEvent` is deliberately separate from `GameEvent` and buffered in its own list (`drain_hole_cards()`), so the server can send those cards only to their owner while public events go to everyone.
 
## 6. Typical flows
 
**Register and sit down**
 
```
POST /register {"name":"alice"}          → 201 {id, name, balance}
WS   /ws
  → {id:1, command:"setAuthentication", userId}     ← success
  → {id:2, command:"joinTable", tableId}            ← success   (worker stores event sender)
  ← {event:"snapshot", seats, board, pot, street, yourSeat, yourCards}   (private)
  → {id:3, command:"table", tableId,
        action:{action:"sit", bankAmount:500}}      ← success   (worker calls Table::sit)
every 1s: worker → Table::process() → GameEvent to each joined connection
```

## 7. Web client (Solid.js)

`client/` is a Solid.js + Vite client for the server above. It registers a
player (or resumes one from `localStorage`), authenticates and joins the table
over the WebSocket, and renders the felt, seats, board, pot, the table stakes,
your hole cards, the hands revealed at showdown and the event stream. It sends `sit`, `stand`, `bet`, `check` and `fold` (with Call as
the difference to the current bet) and reconnects automatically.

```bash
cd client
npm install
npm run dev        # http://localhost:5173
```

Start `pokerserver` first (`cargo run -p pokerserver`); the dev server proxies
`/ws`, `/register`, `/user`, `/usercount` and `/ping` to `localhost:3000`.
Override the proxy target with `POKER_SERVER=http://host:port npm run dev`
(`npm run typecheck` runs the TypeScript check).

For a single server process, build the client into one self-contained HTML file
and install it in the server crate:

```bash
./scripts/build-client.sh    # client build → server/pokerserver/static/client.html
cargo run -p pokerserver     # client and API on http://localhost:3000
```

The script runs `npm ci` when `node_modules` is missing, builds with Vite, and
refuses to copy a bundle that still points at external `/assets` files.
`pokerserver` embeds `static/client.html` with `include_str!` and serves it at
`/`, so `cargo build` needs neither Node nor `client/dist`. The generated file
is checked in; re-run the script and rebuild the server after front-end
changes. `VITE_SERVER_URL` is only needed when the client is hosted on a
different origin from the server.

Every `joinTable`, including each reconnect, is answered with a private
`snapshot` frame, so the felt, seats, board, pot, actor and your own cards are
restored after a reload. After that the client applies the event stream, keeping
your stack in step from your antes, blinds, bets and payouts. A socket that drops is stood up
server-side, so on reconnect the client is a spectator and its player can take a
seat again.

 