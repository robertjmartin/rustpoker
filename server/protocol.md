# Poker server wire protocol

This is the client-facing specification for `pokerserver`. It describes every
HTTP endpoint, WebSocket frame, rule that affects what a client may send, and
the state a client has to derive from the event stream. It was checked against
the code in `pokerserver/src` and `pokercore/src` and against a running server.

The server hosts exactly one table. It listens on `0.0.0.0:3000`; the HTTP base
URL is `http://<host>:3000` and the WebSocket URL is `ws://<host>:3000/ws`.
All state is in memory and is lost when the process restarts.

## 1. Conventions

- All request and response bodies and all WebSocket messages are JSON.
- Field names are `camelCase`. JSON object key order is irrelevant.
- Seats are zero-based indices `0..7` (`u8`). There are always eight seats.
- Chip amounts are unsigned 32-bit integers; a `u32` field rejects negative,
  fractional and string values.
- User IDs are UUID strings (`xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`). The ID is
  the whole identity: there is no password or token, and anyone who knows an ID
  can authenticate as it.
- Unknown JSON fields are ignored, including in the envelope and in nested
  actions. Missing or wrongly typed required fields are not ignored (see §3.4).
- There is no CORS configuration, no cookie or header authentication, and no
  Origin check. A browser client must be served from the same origin as the
  HTTP API (or go through a reverse proxy) to use `/register` and `/user`;
  WebSocket connections from any origin are accepted.

## 2. HTTP API

| Method | Path | Request | Success response |
|---|---|---|---|
| `GET` | `/ping` | – | `200`, body `{"status": "ok"}`, `content-type: text/plain` |
| `POST` | `/register` | `{"name": "<string>"}` | `201`, `User` JSON |
| `GET` | `/user/{id}` | – | `200`, `User` JSON |
| `GET` | `/usercount` | – | `200`, `{"count": <number>}` |
| `GET` | `/` | – | The embedded reference client (HTML). `Cache-Control: no-store` |
| `GET` | `/test` | – | Embedded test page (HTML) |
| `GET` | `/ws` | WebSocket upgrade | `101 Switching Protocols` |

`User`:

```json
{ "id": "6f1c8c1e-...-...", "name": "alice", "balance": 1000 }
```

- `POST /register` requires `content-type: application/json`; anything else is
  `415`. A missing/non-string `name` or malformed JSON is `422` / `400`. An
  empty string is accepted. Names are not unique.
- New users always start with `balance: 1000`. The balance is informational:
  the server never debits or credits it, and `sit` is not checked against it.
- `GET /user/{id}` returns `400` when `{id}` is not a UUID and `404` when it is
  a valid UUID that is not registered.
- `GET /usercount` counts registered users, not connected players.

## 3. WebSocket connection

`GET /ws` upgrades to a WebSocket. No subprotocol is negotiated and no headers,
query parameters or first message are required.

### 3.1 Framing and keepalive

- Every application message is a single JSON object in one **text** frame.
  There is no batching, length prefix, newline framing or handshake message.
- Binary frames are ignored. Only text frames are parsed.
- The server sends a WebSocket **ping** every 10 seconds. On each ping tick it
  checks how long it has been since the last inbound frame and closes the
  socket once that exceeds 25 seconds (in practice about 30 seconds, since the
  check runs on the ping tick). Any inbound frame — including the pong that
  browsers send automatically — resets that clock, so a browser client needs no
  heartbeat of its own. A custom client must answer pings or send a request at
  least every 25 seconds.
- Application-level messages must be read continuously. Each connection has a
  queue of 100 outbound messages; when a client stops reading and that queue
  fills, the server's table worker blocks trying to write to it, which stalls
  the table for everyone. Do not pause reads.
- The server never sends a protocol version or sequence number. Frames are
  ordered per connection as the server enqueued them.

### 3.2 Request envelope

```json
{ "id": 1, "command": "joinTable", "tableId": "00000000-0000-0000-0000-000000000000" }
```

- `id` is required and is a `u32` (`0`–`4294967295`). It is client-chosen and
  echoed in the response. The server does not deduplicate IDs, so reuse is
  allowed; matching is the client's job.
- `command` selects the command. The command's own fields are flattened into
  the same object (there is no nested `payload`).
- `table` carries a nested `action` object (§6).

### 3.3 Response envelope

Every request that parses produces exactly one response, written before the
table events caused by that request. Events caused by other clients and by the
one-second clock can arrive at any time, including between a request and its
response.

Success:

```json
{ "id": 1, "response": "success", "command": "joinTable" }
{ "id": 2, "response": "success", "command": "setAuthentication", "userId": "6f1c..." }
{ "id": 3, "response": "success", "command": "getHoleCards" }
```

Error:

```json
{ "id": 4, "response": "error", "error": "notAuthenticated" }
```

Only two error kinds exist:

| `error` | Meaning |
|---|---|
| `notAuthenticated` | The command needs authentication on this socket and none has been set. |
| `unknown` | `setAuthentication` with an unregistered `userId`, or `getAuthentication` while not authenticated. |

### 3.4 Messages that produce no response

A frame that fails to parse is logged server-side and **ignored**; no response
is sent and the socket stays open. A client must therefore use a timeout to
detect mistakes. This happens for:

- malformed JSON;
- a missing `id`, `command`, `tableId`, `action`, `userId`, `bankAmount` or
  `betAmount`;
- an unknown `command` or unknown `action` value;
- a wrong type (`"id": "5"`, `"betAmount": 1.5`, a negative amount);
- an invalid UUID in `userId` or `tableId` (a syntactically valid UUID is
  required even though `tableId` is ignored, so use a constant such as
  `00000000-0000-0000-0000-000000000000`).

Note that gameplay legality is **not** a parse error: illegal actions are
acknowledged with `success` and then dropped (§6.2).

## 4. Commands

Authentication state is per socket and starts unset on every new connection.

### 4.1 `setAuthentication`

```json
→ { "id": 1, "command": "setAuthentication", "userId": "6f1c..." }
← { "id": 1, "response": "success", "command": "setAuthentication", "userId": "6f1c..." }
← { "id": 1, "response": "error", "error": "unknown" }
```

Binds the socket to a registered user. The response echoes the user ID.

Authenticate exactly once per socket, as the first command. Re-authenticating
on the same socket changes the identity used by later commands but does not
move or clean up any seat or subscription made under the previous identity, and
the eventual disconnect is reported for the new identity. Treat "switch user"
as unsupported and open a new socket instead.

### 4.2 `getAuthentication`

```json
→ { "id": 2, "command": "getAuthentication" }
← { "id": 2, "response": "success", "command": "setAuthentication", "userId": "6f1c..." }
← { "id": 2, "response": "error", "error": "unknown" }
```

Returns the socket's current user. Quirk: the `command` field says
`setAuthentication` even for this command; use the presence of `userId` and the
matching `id` when interpreting the frame. An unauthenticated socket gets
`error: "unknown"` (not `notAuthenticated`).

### 4.3 `joinTable`

```json
→ { "id": 3, "command": "joinTable", "tableId": "00000000-0000-0000-0000-000000000000" }
← { "id": 3, "response": "success", "command": "joinTable" }
← { "event": "snapshot", ... }        // private, immediately after the response
```

Requires authentication. Subscribes this socket to the table's event broadcast
and sends it a private `snapshot` (§8.1). `tableId` is required and ignored —
there is only one table.

`joinTable` is idempotent and is the **resync** operation: sending it again
replaces this user's event channel and sends a fresh snapshot. Nothing else is
reset, and no `leave`/`sit` event is produced. Use it:

- on every connect or reconnect;
- after `sit`, because the `sit` acknowledgement does not include the assigned
  seat and the `sit` event is not addressed to a user (see §6.1 and §13);
- after `leaveTable`, to resume receiving events.

Send `joinTable` before `sit`. A `sit` sent first still seats the player, but
the connection is not in the broadcast set, so the player receives no events
and no hole cards until `joinTable` is sent.

### 4.4 `leaveTable`

```json
→ { "id": 4, "command": "leaveTable" }
← { "id": 4, "response": "success", "command": "leaveTable" }
```

Requires authentication. Stands the player up if seated (equivalent to
`stand`, §6.1) **and removes this socket from the broadcast set**. The socket
stays open and keeps answering commands, but it receives no more table events
until `joinTable` is sent again. Other joined clients see a `leave` event.

### 4.5 `getHoleCards`

```json
→ { "id": 5, "command": "getHoleCards" }
← { "id": 5, "response": "success", "command": "getHoleCards" }
← { "event": "holeCards", "seat": 3, "cards": [ ... ] }   // private, if applicable
```

Requires authentication. Asks the table to resend this player's current hand.
The acknowledgement always follows the standard pattern; the private
`holeCards` frame follows only if this socket has joined the table, the user is
seated, and the user was dealt into the current hand. This is a second way
(besides a fresh snapshot) to learn your seat: the frame contains it.

### 4.6 `table`

```json
→ { "id": 6, "command": "table", "tableId": "00000000-0000-0000-0000-000000000000",
    "action": { "action": "bet", "betAmount": 20 } }
← { "id": 6, "response": "success", "command": "table" }
```

Requires authentication. Queues one table action (§6) for the worker.
`tableId` is required and ignored. `success` only means the action was queued;
it does not mean the action was legal or applied. `joinTable` is not a
precondition: an action sent without joining is still applied by the worker,
but the socket will not see the resulting events or its hole cards.

## 5. Cards

```json
{ "suit": "spades", "rank": "ace" }
```

- `suit` is one of `hearts`, `diamonds`, `spades`, `clubs`.
- `rank` is one of `two`, `three`, `four`, `five`, `six`, `seven`, `eight`,
  `nine`, `ten`, `jack`, `queen`, `king`, `ace`.
- Both are lowercase. The deck is a standard 52-card deck; no deck identity or
  card ID is exposed.

## 6. Table actions

`action` is tagged by its own `action` field:

| `action` | Extra field | Meaning |
|---|---|---|
| `sit` | `bankAmount` (u32) | Take the lowest-numbered free seat with this stack. |
| `stand` | – | Leave the seat. |
| `bet` | `betAmount` (u32) | Add this many chips to your current-round contribution. |
| `check` | – | Pass when nothing is owed. |
| `fold` | – | Fold the hand. |

### 6.1 Seating

- `sit` assigns the **lowest-numbered free seat** (0–7). There is no way to
  choose a seat and the response does not say which seat was assigned.
- `bankAmount` is the starting stack, taken out of nowhere: the user's
  registered `balance` is neither checked nor debited. Any `u32` is accepted,
  including `0`, which creates a seat that is dealt in but can never act.
- Sitting while already seated is ignored (no second seat, no event).
- Sitting while a hand is in progress adds the player as folded for that hand;
  they are dealt in from the next hand.
- `stand` frees the seat. If a hand is in progress the player is folded
  immediately — other clients see only a `leave` event, not a `fold` — their
  chips already in the pot stay there, and the seat is actually removed at the
  next deal. The socket remains subscribed to events, unlike `leaveTable`.
- Standing when not seated, or standing twice, is a silent no-op.
- The same user opening a second socket and sending `joinTable` takes over the
  user's event stream and seat; the older socket stops receiving table events
  and is closed by the server (its late disconnect is ignored).
- Seat ownership is tied to the `UserId` in the worker, not to the socket, so a
  fast reconnect can occasionally retain the seat (see §11).

### 6.2 Betting legality

The three in-hand actions are only applied when **it is that seat's turn**.
On top of that:

| Action | Applied when | Otherwise |
|---|---|---|
| `bet` | It is your turn, you have chips, and `betAmount > 0` after clamping | Ignored |
| `check` | It is your turn and your current-round bet is at least `currentBet` (nothing to call) | Ignored |
| `fold` | It is your turn | Ignored |

Ignored actions produce **no event**; the `success` response was already sent.

`betAmount` is the number of chips to add now, not the total you want your
current-round contribution to become:

- It is clamped to your stack: requesting 500 with 88 behind bets 88 and the
  `bet` event reports 88. Always trust the event amount, not the request.
- It may be smaller than the amount needed to call. There is no minimum bet, no
  minimum raise and no all-in threshold. A short bet leaves you owing the rest
  and you will be asked to act again after the other players.
- `currentBet` becomes the maximum of the old value and your new round total.
  Example: as big blind with 20 already in and `currentBet` 20, betting 20 puts
  you at 40 and raises the price to 40.
- To call, send `betAmount = currentBet - yourCurrentRoundBet` (if that is 0,
  use `check`).
- A bet of `0` is ignored. There is no way to bet more than your stack.

## 7. Server events

Public events are sent to every socket that has joined the table, in the order
the table generated them, one JSON object per text frame. Spectators seated or
not receive the same public stream. Every public event has an `event` field:

```json
{ "event": "sit",   "seat": 3 }
{ "event": "leave", "seat": 3 }
{ "event": "deal",  "seatsDealt": [3, 4] }
{ "event": "ante",  "seat": 3, "amount": 2 }
{ "event": "blind", "seat": 3, "amount": 10, "kind": "small" }
{ "event": "bet",   "seat": 3, "amount": 50 }
{ "event": "check", "seat": 3 }
{ "event": "fold",  "seat": 3 }
{ "event": "turn",  "seat": 3 }
{ "event": "communityCards", "cards": [ { "suit": "spades", "rank": "ace" }, ... ] }
{ "event": "showdown", "hands": [ { "seat": 3, "cards": [ ... ] }, ... ] }
{ "event": "handEnd", "winners": [3], "amount": 250, "showdown": true }
```

| Event | Fields | Emitted when |
|---|---|---|
| `sit` | `seat` | A player takes a seat. |
| `leave` | `seat` | A seated player stands up, disconnects, or sends `leaveTable`. Emitted once per departure. |
| `deal` | `seatsDealt` (array) | A new hand begins; lists every seat dealt in, ascending. Sent before the antes and blinds. |
| `ante` | `seat`, `amount` | Each dealt-in player pays the table ante. Amount already clamped to the stack. Omitted for a player with 0 chips. |
| `blind` | `seat`, `amount`, `kind` (`small`/`big`) | The forced blinds. Omitted when the clamped amount is 0. A blind is live: it counts toward that player's round bet. |
| `bet` | `seat`, `amount` | Chips actually added by a `bet` action. |
| `check` | `seat` | A check action, including an automatic timeout check. |
| `fold` | `seat` | A fold action, including an automatic timeout fold. Standing up or disconnecting is reported as `leave` instead, even though it folds a live hand. |
| `turn` | `seat` | A new decision starts for that seat. |
| `communityCards` | `cards` (array) | A street opens: 3 cards for the flop, then 1 for the turn and 1 for the river. |
| `showdown` | `hands: [{seat, cards: [2]}]` | Immediately before `handEnd`, only when the hand reaches showdown and at least one player was awarded the pot. Contains every player still in the hand in seat order; folded hands are never revealed. |
| `handEnd` | `winners` (seats, ascending), `amount`, `showdown` (bool) | The pot has been awarded. `amount` is the whole pot, not a per-winner share. Folded-out hands have `showdown: false` and no `showdown` frame. |

Ordering rules the client may rely on:

- `deal` → `ante`* → `blind` (small, then big) → `turn`.
- Every `communityCards` frame is followed by a `turn` frame if anyone can act;
  `communityCards` is also the signal that all round bets have been reset.
- `showdown` → `handEnd`.
- No turn event is sent for the all-in run-out, where nobody can act.
- After `handEnd` the table is idle. The next `deal` arrives on a later
  one-second tick (up to about a second later) and may be in the same batch as
  `handEnd` if the clock ticked immediately afterwards.
- If every player is all-in, the remaining streets are dealt together on the
  next tick: up to three `communityCards` frames, then `showdown` and `handEnd`.

Events never carry stacks, the pot, `currentBet`, the dealer button, the street
name, a timer, or a user identity. §10 explains how to derive them.

## 8. Private frames

Each socket is written its own private frames, in addition to the public
broadcast, through the same ordered outbound queue.

### 8.1 `snapshot`

Sent only to the socket that just sent `joinTable`, right after the success
response:

```json
{
  "event": "snapshot",
  "seats": [
    { "seat": 0, "occupied": true,  "stack": 450, "bet": 50, "folded": false },
    { "seat": 1, "occupied": true,  "stack": 500, "bet": 0,  "folded": true  },
    { "seat": 2, "occupied": false, "stack": 0,   "bet": 0,  "folded": false }
  ],
  "board": [ { "suit": "spades", "rank": "ace" } ],
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

| Field | Type | Meaning |
|---|---|---|
| `seats` | array of 8 | One entry per seat, in seat order, including empty seats. |
| `seats[].seat` | u8 | The seat index. |
| `seats[].occupied` | bool | False for an empty seat and for a player who stood up mid-hand. |
| `seats[].stack` | u32 | Chips behind, excluding the current street's bet. 0 for unoccupied seats. |
| `seats[].bet` | u32 | Contribution in the current betting round. 0 for unoccupied seats; can be a leftover value of the last hand while `street` is `idle`. |
| `seats[].folded` | bool | Folded this hand, or seated mid-hand and waiting for the next deal. |
| `board` | array of cards | Community cards dealt so far (0–5). |
| `pot` | u32 | Chips already collected, including the current street's bets. |
| `currentBet` | u32 | Highest current-round contribution; the call price. 0 at the start of each postflop street, but may still hold the last street's price while `street` is `idle`. |
| `smallBlind`, `bigBlind`, `ante` | u32 | The table's fixed forced bets (currently 10, 20, 2). |
| `acting` | seat or `null` | Seat to act, `null` when no hand is running or nobody can act. |
| `dealer` | seat or `null` | Dealer button, `null` before the first hand. |
| `street` | string | `idle`, `preFlop`, `flop`, `turn`, `river`. |
| `yourSeat` | seat or `null` | This socket's seat, `null` if not seated (or already stood up). |
| `yourCards` | 2 cards or `null` | Only this socket's cards; `null` when not seated or not dealt in. While `street` is `idle` it can still hold the cards of the hand that just ended. |

Only `pot`, `acting` and `street` are reliably reset when a hand ends. `dealer`
keeps the last button, and `board`, `seats[].bet`, `currentBet`, `folded` and
`yourCards` can retain values from the finished hand until the next `deal`. A
client should ignore the board, bets, the call price and `yourCards` whenever
`street` is `idle` (or `acting` is `null`), rather than treating them as a live
hand.

A snapshot is pushed only in response to `joinTable`; there is no automatic
re-snapshot after sitting, standing, a hand ending or a reconnect. Send
`joinTable` again whenever you need to resynchronise.

The snapshot is authoritative for everything it contains, but it is a
point-in-time view: events generated after it (including ones from commands you
sent after `joinTable`, and from other players) follow it in order. There are
no sequence numbers, so apply frames strictly in arrival order.

### 8.2 `holeCards`

```json
{ "event": "holeCards", "seat": 3, "cards": [ { "suit": "spades", "rank": "ace" }, { "suit": "hearts", "rank": "ten" } ] }
```

Sent only to the player holding the seat: once per deal, right after the public
`deal`/forced-bet frames are queued, and again in reply to `getHoleCards`. Only
the two-card hand; no board or other players' cards are included. If the
player is not seated or was not dealt in (sitting out the running hand), no
frame is sent.

## 9. Game rules the client must model

These are enforced by the server, but a client needs them to render correctly
and to decide what to enable in its UI.

- **Table size:** 8 seats, fixed stakes: small blind 10, big blind 20, ante 2.
- **Hand start:** on a one-second tick, when at least two players are seated
  (seats flagged as left do not count). The dealer button moves to the next
  occupied seat after the previous dealer, or to the first occupied seat
  scanning from seat 0 when no hand has run yet.
- **Deal and forced bets:** every dealt-in player gets two hole cards, then
  pays the ante (dead money: it goes into the pot but is not part of the
  current-round bet). The two seats after the button post the small and big
  blind. Antes and blinds are clamped to the player's stack.
- **Heads-up** (two players): the button posts the small blind, the other seat
  posts the big blind, and the button acts first preflop. Postflop the
  non-button acts first.
- **Three or more:** the seat after the button posts the small blind, the next
  posts the big blind, and preflop action starts with the seat after the big
  blind.
- **Opening price:** preflop `currentBet` is the nominal big blind (20) even if
  the posting player was short, so there is always something to call. Postflop
  each street starts with `currentBet` 0 and action starts with the seat after
  the button.
- **Round completion:** a street ends when every player who still has chips has
  acted at least once in that round and has matched `currentBet`. All-in
  players are skipped. A short bet leaves the bettor due to act again.
- **Streets:** preflop → flop (3 cards) → turn (1) → river (1) → showdown.
- **Showdown:** best five of seven cards; ties split the pot. Any remainder
  (pot mod winners) goes to the first winner in ascending seat order.
- **Fold-out:** when everyone else folds, the last player takes the pot with no
  `showdown` frame.
- **No side pots:** a short all-in can win the entire pot, and uncalled chips
  are not returned.
- **Turn clock:** each decision gives the actor 30 seconds, counted in
  one-second ticks. On expiry the table automatically checks if nothing is owed,
  otherwise folds. These appear as ordinary `check`/`fold` events; there is no
  timeout event and the deadline is not published.
- **Sitting out:** a player seated mid-hand is folded for that hand and dealt in
  from the next one.
- **Disconnect:** closing the socket stands the player up (once the close or
  the 25-second keepalive timeout is detected). Chips already in the pot are
  forfeited to the hand.
- **Pace:** the table advances only on the one-second clock and on actions.
  There is no continuous state broadcast.

Known server-side stall (not fixable from a client): if a seated player stands
up or disconnects while it is their turn in a hand with three or more players,
that seat is folded but the turn is not passed, so the hand can freeze forever
with no further `turn` events. A heads-up hand instead ends on the next tick.
Clients should be prepared to show a hand with no legal actor rather than
assuming the stream is broken.

## 10. State a client must derive

Public events are deltas, not state snapshots. Starting from a snapshot (or an
empty table plus `deal`), maintain:

- **Pot:** add every `ante`, `blind` and `bet` amount. Zero it on `handEnd`
  (whose `amount` is the total that was paid out) and on `deal`.
- **Per-seat round bet:** add `blind` and `bet` amounts per seat. Reset every
  seat's bet to 0 when a `communityCards` frame arrives, and on `handEnd` and
  `deal`. Antes never count.
- **`currentBet`:** the maximum per-seat round bet; reset to 0 on
  `communityCards` and at the end of the hand (the server may leave a stale
  value in an idle snapshot); preflop it starts at 20 (or read it from the
  snapshot while a hand is running). A `bet` event of amount `a` for a seat
  means that seat's total became `oldBet + a`, and `currentBet` becomes
  `max(currentBet, oldBet + a)`.
- **Board:** append the cards of each `communityCards` frame and clear them on
  `deal`. Ignore a board from an idle snapshot; it can be the previous hand's.
- **Seat flags:** on `deal`, set each occupied seat's `folded` to whether that
  seat is absent from `seatsDealt`. On `leave`, clear the seat's occupancy and
  last action and treat it as folded for the running hand.
- **Actor:** the last `turn` event's seat; clear it on `handEnd` and when
  `communityCards` has no following `turn` (all-in run-out).
- **Street:** derive from the board length (0 cards preflop, 3 flop, 4 turn,
  5 river) or from the snapshot.
- **Your stack:** snapshot `seats[yourSeat].stack`, minus your own `ante`,
  `blind` and `bet` events, plus your share of `handEnd.amount` when you are a
  winner (`floor(amount / winners.length)`, plus the remainder if you are the
  first winner in the ascending list).
- **Your cards:** `yourCards` in the snapshot when a hand is running and
  `holeCards` frames. Clear them on `deal` unless a `holeCards` frame for the
  new hand follows, and on `leave` of your seat. Do not show `yourCards` from a
  snapshot whose `street` is `idle`; it may be the previous hand's cards.
- **Revealed hands:** keep `showdown.hands` for the seats and clear them on the
  next `deal`.

The reference client in `client/` is a working model of these rules.

## 11. Connection lifecycle and reconnect

1. `POST /register` → save `id`.
2. Open `ws://<host>:3000/ws`.
3. `setAuthentication` with that ID.
4. `joinTable` with any valid UUID → receive `snapshot`.
5. `table`/`sit` to take a seat → receiving `snapshot` again (or acting on the
   next `deal`) is how you learn `yourSeat`.
6. Play until the socket closes.

Authentication, the event subscription and the seat are separate:

- Authentication is per socket. A new socket starts unauthenticated.
- The event subscription is per socket. `leaveTable` removes it; `joinTable`
  restores it; closing the socket ends it.
- The seat belongs to the `UserId` in the table worker. Closing a socket sends
  a disconnect for that user; usually the player is stood up, so a reconnecting
  client is a spectator and must `sit` again. If the new socket's `joinTable`
  is processed before the old socket's disconnect, the worker treats the
  disconnect as stale and keeps the connection and seat. Either outcome is
  possible, so trust `snapshot.yourSeat` after reconnecting rather than
  assuming the seat was kept or lost.
- There is no resume token, hand history or event replay. A reconnecting
  client starts from the snapshot; past events are gone.
- If the server process restarts, every user ID and seat is gone (the registry
  is in memory), so re-register.

## 12. Complete example

Two players, one hand. Frames are in wire order; `A` and `B` are separate
sockets. Events are broadcast to both unless marked private.

```text
POST /register {"name":"alice"}
→ 201 {"id":"a1...","name":"alice","balance":1000}
POST /register {"name":"bob"}
→ 201 {"id":"b2...","name":"bob","balance":1000}

A → {"id":1,"command":"setAuthentication","userId":"a1..."}
A ← {"id":1,"response":"success","command":"setAuthentication","userId":"a1..."}
B → {"id":1,"command":"setAuthentication","userId":"b2..."}
B ← {"id":1,"response":"success","command":"setAuthentication","userId":"b2..."}

A → {"id":2,"command":"joinTable","tableId":"00000000-0000-0000-0000-000000000000"}
A ← {"id":2,"response":"success","command":"joinTable"}
A ← {"event":"snapshot","seats":[8 empty seats],"board":[],"pot":0,"currentBet":0,
     "smallBlind":10,"bigBlind":20,"ante":2,"acting":null,"dealer":null,
     "street":"idle","yourSeat":null,"yourCards":null}
B → {"id":2,"command":"joinTable","tableId":"00000000-0000-0000-0000-000000000000"}
B ← {"id":2,"response":"success","command":"joinTable"}
B ← {"event":"snapshot", ... same table ...}

A → {"id":3,"command":"table","tableId":"...","action":{"action":"sit","bankAmount":500}}
A ← {"id":3,"response":"success","command":"table"}
both ← {"event":"sit","seat":0}
B → {"id":3,"command":"table","tableId":"...","action":{"action":"sit","bankAmount":500}}
B ← {"id":3,"response":"success","command":"table"}
both ← {"event":"sit","seat":1}

A → {"id":4,"command":"joinTable","tableId":"..."}     # learn yourSeat
A ← {"id":4,"response":"success","command":"joinTable"}
A ← {"event":"snapshot", ..., "seats":[{"seat":0,"occupied":true,"stack":500,...}],
     "yourSeat":0,"yourCards":null}

--- next one-second tick starts the hand (heads-up: seat 0 is button/SB) ---
both ← {"event":"deal","seatsDealt":[0,1]}
both ← {"event":"ante","seat":0,"amount":2}
both ← {"event":"ante","seat":1,"amount":2}
both ← {"event":"blind","seat":0,"amount":10,"kind":"small"}
both ← {"event":"blind","seat":1,"amount":20,"kind":"big"}
both ← {"event":"turn","seat":0}
A ← {"event":"holeCards","seat":0,"cards":[{"suit":"diamonds","rank":"nine"},
                                            {"suit":"diamonds","rank":"king"}]}   # private
B ← {"event":"holeCards","seat":1,"cards":[{"suit":"spades","rank":"nine"},
                                            {"suit":"spades","rank":"queen"}]}    # private

A → {"id":5,"command":"table","tableId":"...","action":{"action":"bet","betAmount":10}}
A ← {"id":5,"response":"success","command":"table"}   # call: 20 - 10
both ← {"event":"bet","seat":0,"amount":10}
both ← {"event":"turn","seat":1}

B → {"id":4,"command":"table","tableId":"...","action":{"action":"check"}}
B ← {"id":4,"response":"success","command":"table"}
both ← {"event":"check","seat":1}
both ← {"event":"communityCards","cards":[3 flop cards]}
both ← {"event":"turn","seat":1}                      # bets now 0, price 0

... turn and river checks ...
both ← {"event":"communityCards","cards":[1 river card]}
both ← {"event":"turn","seat":1}
...
both ← {"event":"showdown","hands":[{"seat":0,"cards":[...]},{"seat":1,"cards":[...]}]}
both ← {"event":"handEnd","winners":[0],"amount":54,"showdown":true}
```

A player who does not act for 30 seconds produces an ordinary
`{"event":"check","seat":n}` or `{"event":"fold","seat":n}` and the hand moves
on, or a closed socket produces
`{"event":"leave","seat":n}` followed by `handEnd` if that ends the hand.

## 13. Protocol quirks to code around

These are current server behaviour, not suggestions:

1. **No seat in the `sit` acknowledgement**, and `sit` events carry no user.
   Re-send `joinTable` after sitting (or wait for your `holeCards` frame) to
   learn `yourSeat`. Do not assume the next `sit` event you see is yours.
2. **`getAuthentication` reports `command: "setAuthentication"`.**
3. **`leaveTable` stops the event stream** for that socket. Re-join afterwards.
4. **Illegal gameplay actions are acknowledged and dropped**, with no event and
   no error. Enable only actions you believe are legal, and verify by watching
   the stream.
5. **A frame that fails to parse gets no response and no close.** Use request
   timeouts.
6. **No bet-reset event.** Reset displayed bets when `communityCards` arrives.
7. **No pot, stack, actor or street fields in events.** Derive them (§10) or
   re-snapshot.
8. **`yourCards` is `null` for a player seated mid-hand** until the next deal.
9. **`0` is a valid stack** and produces a seat that never acts.
10. **No action result:** `success` for `table` is only a queue acknowledgement.
11. **No per-request id in events**, so a client cannot correlate an event to
    its request; the `id` field exists only on responses.
12. **`tableId` is parsed but ignored**, and must still be a valid UUID.
13. **No CORS headers**, so browser HTTP calls need same-origin or a proxy
    (the reference client's Vite config proxies `/register`, `/user`,
    `/usercount`, `/ping` and `/ws` to `localhost:3000`).
14. **No table list, chat, hand history, run-it-twice or seat choice.**

## 14. Client implementation checklist

- [ ] Register over HTTP, or resume a stored user ID, and handle `404` by
      registering again.
- [ ] Open `/ws`, authenticate, join, and keep the socket's read loop active.
- [ ] Render eight seats from `snapshot.seats` and keep `mySeat`, `myCards`,
      board, pot, bets, actor and street in sync from events.
- [ ] Re-snapshot on every (re)connect and after `sit`.
- [ ] Enable `bet`/`check`/`fold` only when `turn.seat === mySeat`; offer
      `check` only when `currentBet === myBet`; compute a call as
      `currentBet - myBet` capped at the stack.
- [ ] Send integer `u32` amounts and a valid constant `tableId` with every
      `table` command.
- [ ] Handle `handEnd` by clearing the actor, applying winnings, and waiting for
      the next `deal` (which resets the board and pot).
- [ ] Treat unknown or malformed input defensively: the server never sends a
      protocol error frame, so a silent request means a parse failure.
