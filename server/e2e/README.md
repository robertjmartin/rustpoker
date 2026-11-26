# pokerserver end-to-end tests

Black-box tests for `pokerserver`. Each test launches a real server process and
drives it the way a browser client does: HTTP for accounts and WebSocket for the
table protocol. There are no mocking layers and no extra dependencies — Node 22+
provides `fetch`, `WebSocket` and the test runner.

When server behavior changes, follow
[`.agents/skills/pokerserver-e2e/SKILL.md`](../../.agents/skills/pokerserver-e2e/SKILL.md):
scenarios, tier membership and the docs in this file are updated together.

## Running

```bash
cd server/e2e
npm test                # dev tier (fast loop, 5-10s target)
npm run test:dev        # same as npm test
npm run test:checkin    # once before committing (at most 5 minutes)
npm run test:full       # CI only, no time limit
```

`npm test` is an alias for `test:dev`. The runner rebuilds `pokerserver`
before every run, so the binary under test always matches the sources. CI runs
`npm run test:full` from `.github/workflows/ci.yml`.

## Frequency tags

Test files group tests by area, but tags say *when* a test runs. Every test has
exactly one tag, and higher tiers include lower ones:

| Tag | npm script | Contents | Budget |
|---|---|---|---|
| `dev` | `npm run test:dev` | HTTP surface, auth handshake, one dealt hand (12 tests) | 5–10s |
| `checkin` | `npm run test:checkin` | every test today: dev + protocol, table actions, capacity, disconnects, isolation (22 tests) | at most 5 minutes |
| `full` | `npm run test:full` | same tests as checkin today; reserved for long CI-only scenarios | no limit |

`TAG_LEVELS` in `src/framework/test.js` is the single list of available tags,
and `package.json` has one explicit script per tag. Adding a tag is
deliberately a two-place change: add it to `TAG_LEVELS` and add its npm script.
Tags describe how often a test runs, not its feature area, so a new area goes
in a new test file rather than a new tag.

Each test declares its tier:

```js
e2eTest('two players are dealt a hand', TAGS.DEV, async (t) => { ... });
```

`node scripts/run.js <tag>` runs a tier directly. Deselected tests are not
registered at all, so they produce no output and do not appear in the counts.
The runner imports the test files once (without `node:test`) to learn which
files contain a selected test, then starts only those files for the real run.

### What each tier is for

- **dev** — the loop while you are editing; run it constantly. It should cover
  broad functionality without deep edge cases, and the whole run should stay in
  the 5–10 second range. If a new test would push it past that, put it in a
  slower tier instead of trading away dev's speed.
- **checkin** — run locally once before committing, while the work is still
  fresh. It may take up to 5 minutes. This is the ordinary correctness floor:
  protocol behaviour, capacity and disconnect handling belong here unless they
  are too slow.
- **full** — CI only. It may take as long as it needs. Use it for slow, deep
  or environment-heavy scenarios (soak runs, many clients, repeated runs) that
  would make checkin exceed its 5-minute budget.

### Initial shape and how to maintain it

While the suite is small, `checkin` contains every test. Only the fastest, most
important tests are also tagged `dev`, and no test is `full`-only yet.
`full` currently runs the same set as `checkin`; it exists so slow tests
have an obvious home as the library grows.

When adding a test:

1. Start at `dev` only if it covers a broad, important path and keeps the dev
   run under 10 seconds.
2. Otherwise tag it `checkin`.
3. Tag it `full` directly if it is inherently slow or deep; do not push
   `checkin` past 5 minutes.

Keep the invariants explicit: every test has exactly one tag, `dev` and
`checkin` stay inside their budgets, and `full` remains the catch-all for
work that is too slow for a local pre-commit run.

## Layout: framework vs tests

```
server/e2e/
├── scripts/run.js          build, pick test files with a selected tag, run them
├── src/framework/
│   ├── server.js           spawn/stop a clean server, discover its port, tick it
│   ├── http.js             /ping /register /user/{id} /usercount helpers
│   ├── client.js           WS client: id-correlated requests, ordered push stream
│   ├── player.js           high level: register + authenticate + joinAndSit
│   ├── test.js             TAG_LEVELS registry and tag-selecting e2eTest
│   └── index.js            the only module tests import
└── src/tests/
    ├── http.test.js          endpoints
    ├── auth.test.js          authentication handshake
    ├── gameplay.test.js      dealing (dev) and table actions (checkin)
    ├── capacity.test.js      eight-seat table (checkin)
    ├── protocol.test.js      malformed input, test-hook gating (checkin)
    ├── resilience.test.js    disconnects (checkin)
    └── isolation.test.js     parallel servers keep separate state (checkin)
```

Scenarios contain only user-story steps and assertions. If a test needs a new
server process, protocol detail, wait or port, that belongs in `src/framework`.

## How a server is launched

- `startServer()` runs `server/target/debug/pokerserver` with `POKER_PORT=0`.
  The OS assigns a free port, and the harness reads it from the server's
  `listening on …:<port>` line. Parallel test files therefore cannot collide.
- The process gets `POKER_TICK_MS=60000` by default, so the table never ticks on
  its own during a test. `server.tick()` sends `POST /test/tick`, which runs
  exactly one `Table::process()` tick. That removes sleep-based flakiness:
  seat everyone, tick, then assert the deal.
- That route only exists when the server starts with `POKER_TEST_HOOKS=1`
  (off by default, set by the harness).
- `launchServer(t)` registers shutdown via `t.after`; clients do the same, so a
  failing assertion never leaks a process or socket.
- The last 300 lines of server stdout/stderr stay in `server.output` and appear
  in request-timeout errors.

## Parallelism

`node --test` runs each test file in its own process, and every server has its
own port and in-memory user registry. Within a file, tests run sequentially and
each starts its own server, so ordering dependencies cannot creep in. Keep new
tests independent; do not share a server or table between tests.

## Adding a test

```js
import assert from 'node:assert/strict';
import { TAGS, e2eTest } from '../framework/test.js';
import { createPlayer, launchServer } from '../framework/index.js';

const TAG = TAGS.DEV;

e2eTest('a seated player receives a sit broadcast', TAG, async (t) => {
  const server = await launchServer(t);
  const alice = await createPlayer(t, server, 'alice');
  await alice.joinAndSit(500);

  const sit = await alice.client.waitForEvent('sit');
  assert.equal(sit.seat, 0);
});
```

Put the test in the file for its area. Tag it `DEV` only when it is essential,
deterministic and small enough to keep the fast loop at 5–10 seconds; use
`CHECKIN` for ordinary correctness and `FULL` for slow or deep scenarios.

Rules of thumb:

- Use `waitForEvent` / `waitFor` instead of `setTimeout`; a fixed sleep hides
  races. `expectNoEvent` / `expectNoResponse` are for negative assertions.
- One server per test. If a scenario needs a different tick interval, pass
  `{ tickMs }` to `launchServer`.
- Choose one frequency tag per test. Do not add an area tag; areas are files.
- Keep `dev` under 10 seconds and `checkin` under 5 minutes. Promote tests to a
  slower tier rather than relaxing either budget.
- Adding a new frequency tag means editing `TAG_LEVELS` and adding an explicit
  npm script, on purpose.

## What is not covered yet

The core has no betting/fold/stand rules and the server ignores `tableId`, so
the suite pins current behaviour: those actions are acknowledged but do not
change the hand, and a disconnected player's seat is not freed.
