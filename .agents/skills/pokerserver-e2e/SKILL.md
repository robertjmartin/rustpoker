---
name: pokerserver-e2e
description: Maintain pokerserver's end-to-end tests and their documentation when server behavior changes. Use when adding or changing an HTTP endpoint, WebSocket command, table action, game event, seating/capacity rule, test tier, or when an e2e test fails.
---

# pokerserver end-to-end test maintenance

The suite lives in `server/e2e`. It is black box: every test starts a real
`pokerserver` process and speaks HTTP/WebSocket. When the server contract
changes, tests **and** test documentation change in the same piece of work.

## Orientation

- `server/e2e/README.md` — frequency tiers, budgets, framework layout, how a
  server is launched.
- `.github/workflows/ci.yml` — CI: Rust unit tests plus `npm run test:full`.
- `server/e2e/src/framework/` — process lifecycle, HTTP, WebSocket client,
  player helper, tag registry. Tests import only `src/framework/index.js` and
  `src/framework/test.js`.
- `server/e2e/src/tests/*.test.js` — scenarios grouped by area.
- `README.md` (repository root) §3–§4 — documented protocol and internal
  interfaces.
- `server/pokerserver/src/{main,client_command,client_responses,wire,table_command}.rs`
  — server side of the contract.
- `server/pokercore/src/{table,game_event}.rs` — game rules and emitted events.

## Invariants

1. **Black box only.** Never import Rust crates, mock the server, or reach into
   its state. Drive it over HTTP/WebSocket.
2. **Framework vs scenarios.** Spawning, ports, timeouts, waits and protocol
   encoding belong in `src/framework`. `src/tests` contains scenario steps and
   assertions only.
3. **One server per test.** Use `launchServer(t)`; one socket per player via
   `createPlayer(t, server, name)`. Both register `t.after` cleanup.
4. **No sleeps for state transitions.** `server.tick()` runs one table tick
   deterministically. Wait on events with `waitForEvent` / `waitFor`; reserve
   `expectNoEvent` / `expectNoResponse` for negative assertions.
5. **Tags are frequency tiers, one per test:** `dev`, `checkin`, `full`.
   Files group by area. Do not add feature tags; a new area is a new file.
6. **Tags have two homes by design.** `TAG_LEVELS` in
   `src/framework/test.js` is the single list; `package.json` has one explicit
   script per tag. Adding a tag means editing both, plus docs.
7. **No npm dependencies.** Node 26+ built-ins (`fetch`, `WebSocket`,
   `node:test`) are deliberate.
8. **Tracing is opt-in per test.** `e2eTest(..., { trace: true })` writes
   `traces/<test-name>.log` with HTTP and WebSocket traffic. Leave it off in
   committed tests unless the test is deliberately meant to trace.

## Workflow when server behavior changes

1. **State the contract delta.** Update the repository root `README.md` §3
   (HTTP, WebSocket envelopes, commands, events) and §4 (internal types) before
   or alongside the code. Then make the test assert that contract.
2. **Update or add the scenario** in the matching area file:

   | Area | File |
   |---|---|
   | HTTP endpoints | `src/tests/http.test.js` |
   | authentication handshake | `src/tests/auth.test.js` |
   | seating, dealing, table actions | `src/tests/gameplay.test.js` |
   | eight-seat capacity | `src/tests/capacity.test.js` |
   | malformed input, test hooks | `src/tests/protocol.test.js` |
   | disconnects | `src/tests/resilience.test.js` |
   | parallel server isolation | `src/tests/isolation.test.js` |

   A genuinely new area gets a new `*.test.js` file; update the layout block in
   `server/e2e/README.md`.
3. **Extend the framework only when needed.** If the protocol has a new
   primitive, add it to `client.js` / `http.js` / `player.js` and export it from
   `src/framework/index.js`. Scenario files must not invent protocol details.
4. **Choose the frequency tier** using the budgets in
   `server/e2e/README.md`:
   - `dev` — run constantly while coding; broad, critical paths; whole dev run
     stays in 5–10s.
   - `checkin` — run once locally before committing; ordinary correctness; at
     most 5 minutes. Currently contains every test.
   - `full` — CI only, no time limit; slow, deep or environment-heavy work.
5. **Update the test documentation in the same change:**
   - `server/e2e/README.md`: tier table counts, layout if a file was added,
     "what is not covered yet", and anything that describes the changed
     behavior.
   - `src/framework/test.js`: header comment if tier rules or membership
     change.
   - `README.md` §7: only if commands, environment variables or setup change.
   - If a tag is added: `TAG_LEVELS`, the matching `package.json` script, and
     both READMEs.
6. **Run the affected checks:**

   ```bash
   cd server/e2e
   npm test                # dev tier
   npm run test:checkin    # everything today
   npm run test:full       # same set today; run when touching the framework
   ```

   For Rust changes also run `cd server && cargo test --workspace`. Fix the
   cause of any failure; never skip, weaken or delete assertions to get green.
   If a check genuinely cannot run, report that as a limitation.
7. **Recheck the tier table.** If test membership changed, update the counts
   and keep `dev` under 10s and `checkin` under 5 minutes.

## Adding a test

```js
import assert from 'node:assert/strict';
import { TAGS, e2eTest } from '../framework/test.js';
import { createPlayer, launchServer } from '../framework/index.js';

const TAG = TAGS.DEV; // or TAGS.CHECKIN / TAGS.FULL

e2eTest('two players are dealt a hand', TAG, async (t) => {
  const server = await launchServer(t);
  const alice = await createPlayer(t, server, 'alice');
  const bob = await createPlayer(t, server, 'bob');

  await alice.joinAndSit(500);
  await bob.joinAndSit(500);

  await server.tick();
  assert.deepEqual((await alice.client.waitForEvent('deal')).seatsDealt, [0, 1]);
});
```

Checklist:

- [ ] Scenario is in the file for its area and asserts observable protocol
      output, not server internals.
- [ ] Exactly one frequency tag, chosen against the tier budgets.
- [ ] No fixed ports, sleep loops, shared servers or ordering dependencies.
- [ ] Negative assertions use `expectNoEvent` / `expectNoResponse` with a
      bounded wait.
- [ ] `server/output` is the debugging path for a timeout; request errors
      already include it. For a full wire log, see `{ trace: true }` in the e2e
      README.
- [ ] Test docs updated (counts, coverage notes, layout, contract sections).
- [ ] `npm test` and `npm run test:checkin` pass.

## Documentation map

| Test change | Documentation to update |
|---|---|
| new/renamed test file | `server/e2e/README.md` layout block |
| tier membership change | tier table in `server/e2e/README.md`, counts and budgets |
| new tag | `TAG_LEVELS`, `package.json` script, both READMEs |
| new HTTP/WS behavior | root `README.md` §3, e2e README coverage notes |
| new internal command/event | root `README.md` §4, e2e README coverage notes |
| new env/test hook | `README.md` §7 table, e2e README launch section |
| new script/command | `README.md` §7, e2e README running section, CI workflow |

## Known current limits

These describe behavior the tests pin today; update the tests **and** this list
when the server implements more:

- `bet`, `fold` and `stand` are acknowledged but not applied by the worker.
- `tableId` is parsed but ignored.
- A disconnect does not free the player's seat or remove the connection.
- The table only deals from `Idle`; betting rounds are placeholders.
