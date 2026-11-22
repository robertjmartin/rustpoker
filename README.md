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
                      │  - 5s timer → table.process()     │
                      │  - broadcasts GameEvents          │
                      └───────────────────────────────────┘
```
 
Design points:
 
- **Single-owner table.** The `Table` is owned by one `worker` task. It is never shared behind a lock. All mutations arrive as `TableCommand` messages on an `mpsc` channel, so game state needs no synchronisation.
- **Per-connection tasks.** Each WebSocket gets its own task (`handle_socket`) and its own `mpsc::channel<GameEvent>`. The sender half is handed to the worker on `Join`, and the worker pushes events through it.
- **Tick-driven game loop.** The worker also runs an `interval`, which calls `Table::process()` to advance the state machine and then sends events to all connected clients.
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
- A message that fails to parse is logged server-side and **no response is sent**.
- `getAuthentication` currently reports success using the `setAuthentication` command name in the response.
#### Server-pushed events
 
The worker pushes `GameEvent`s to joined clients through each connection's channel. At present only `GameEvent::DummyEvent` is forwarded, and it is written to the socket as the plain text frame `dummy event` (not JSON). All other event variants are dropped by `handle_socket`. A structured JSON encoding for events is the main missing piece of the client protocol.
 
## 4. Internal interfaces
 
### 4.1 Server-internal messages (`table_command.rs`)
 
The contract between the connection tasks and the table worker:
 
```rust
pub struct TableCommand {
    pub user_id: UserId,
    pub cmd: TableCommands,
}
 
pub enum TableCommands {
    Join(JoinTableCommand),      // carries the connection's event sender
    Leave,
    Action(ActionTableCommand),  // carries a TableAction
}
 
pub struct JoinTableCommand { pub tx: mpsc::Sender<GameEvent> }
pub struct ActionTableCommand { pub action: TableAction }
```
 
Channels:
 
| Channel | Direction | Payload | Capacity |
|---|---|---|---|
| `AppState.table_tx` → worker `rx` | connections → worker | `TableCommand` | 100 |
| per-connection `tx` → `handle_socket` `rx` | worker → connection | `GameEvent` | 100 |
 
Worker-local state: `connections: HashMap<UserId, Sender<GameEvent>>` and `seat_map: HashMap<UserId, u8>` (user to seat index, populated on a successful `sit`).
 
The worker exits when all `table_tx` senders are dropped (`rx.recv()` returns `None`).
 
### 4.2 Wire types (`client_command.rs`, `client_responses.rs`)
 
| Type | Role |
|---|---|
| `UserId(Uuid)` | Identity, serialisable, used as a map key. |
| `TableId(Uuid)` | Table identifier (deserialize only). |
| `ClientMessage { id, command }` | Request envelope. |
| `ClientCommand` | `SetAuthentication`, `GetAuthentication`, `JoinTable`, `LeaveTable`, `Table`. |
| `TableAction` | `Sit`, `Stand`, `Bet`, `Fold`. Reused directly inside `TableCommand`, so the wire type is also the internal type. |
| `ClientMessageResponse { id, response }` | Response envelope. |
| `ClientResponse` | `Success(ClientCommandResponse)` or `Error(ClientCommandError)`. |
| `ClientAuthResponse` | Defined but not yet used. |
 
Request handling is centralised in `process_client_command(state, &mut auth_user_id, &tx, cmd) -> ClientResponse`. Authentication state is per connection and held as `Option<UserId>` in `handle_socket`.
 
## 5. Game core interface (pokercore)
 
### 5.1 Public API
 
`lib.rs` currently exports only:
 
```rust
pub use game_event::{GameEvent, DealEvent, BetEvent};
pub use table::Table;
```
 
`deck` and `table` are private modules, so `Deck`, `Card`, `SeatId` and `BetAmount` are not visible to consumers yet.
 
### 5.2 `Table`
 
```rust
pub struct Table { /* state, users, seats, deck, dealer_pos, current_pos, cards[5] */ }
 
impl Table {
    pub fn new(size: u8) -> Table;                         // size = number of seats
    pub fn player_count(&self) -> u8;                      // occupied seats
    pub fn sit(&mut self, bank: u32) -> Option<u8>;        // seat index, None if full
    pub fn leave(&mut self, seat: u8) -> Option<u32>;      // marks seat as left, returns bank; None if empty
    pub fn bet(&mut self, seat: u8, bet_amount: u32) -> Option<u32>; // returns remaining bank
    pub fn process(&mut self);                             // advance state machine (called on a timer)
}
```
 
Internal model:
 
- `Seat { bank, bet, pot, left, cards: [Option<Card>; 2] }`
- `TableState`: `Idle`, `Bet1` … `Bet5` (betting-round placeholders)
- `Table.cards: [Option<Card>; 5]` for community cards (not yet dealt)
- `Table.users: Vec<Option<User>>` where `User` holds `game_event_callback` and `hole_card_callback` function pointers. This is an early sketch of the event delivery design and is not used by the server, which uses channels instead.
Lifecycle implemented so far:
 
1. `Idle`: remove seats flagged `left` with no outstanding `bet`/`pot`; if more than one player is seated, shuffle a new deck, deal two hole cards to each seat, move to `Bet1`.
2. `Bet1`–`Bet5`: empty.
Seat removal is deferred (`leave` only sets a flag) so that a player who leaves mid-hand can still have chips in the pot.
 
### 5.3 Deck and cards (`deck.rs`, internal)
 
```rust
pub enum Rank { Two = 2, … Ace = 14 }     // Ord
pub enum Suit { Hearts, Diamonds, Spades, Clubs }
pub struct Card { suit, rank, deck_id }   // Card::get_value() -> (Rank, Suit)
pub struct Deck { cards, deck_id }        // new(), shuffle(), count(), deal() -> Option<Card>
```
 
Each `Deck` gets a unique `DeckId` from a global atomic counter, and every `Card` records which deck it came from. Shuffling uses `rand::thread_rng`.
 
### 5.4 Events (`game_event.rs`)
 
```rust
pub enum GameEvent {
    Sit(SitEvent),      // { seat }
    Deal(DealEvent),    // { seats_dealt }
    Bet(BetEvent),      // { seat, amount }
    Fold(FoldEvent),    // { seat }
    DummyEvent,         // placeholder used by the server today
}
 
pub struct HoleCardEvent { cards: [Card; 2] }   // private to the player, not yet part of GameEvent
```
 
Events are the intended output of the core: the table emits them and the server fans them out to clients. `HoleCardEvent` is deliberately separate from `GameEvent` so that hole cards can be sent only to their owner while public events go to everyone.
 
## 6. Typical flows
 
**Register and sit down**
 
```
POST /register {"name":"alice"}          → 201 {id, name, balance}
WS   /ws
  → {id:1, command:"setAuthentication", userId}     ← success
  → {id:2, command:"joinTable", tableId}            ← success   (worker stores event sender)
  → {id:3, command:"table", tableId,
        action:{action:"sit", bankAmount:500}}      ← success   (worker calls Table::sit)
every 5s: worker → Table::process() → GameEvent to each joined connection
```

 