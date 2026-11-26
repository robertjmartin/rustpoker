// Area: playing at the table. This file mixes buckets on purpose: the deal
// path is essential (quick + full), the acknowledged-but-not-applied table
// actions are full-only.
import assert from 'node:assert/strict';
import { TAGS, e2eTest } from '../framework/test.js';
import { createPlayer, launchServer } from '../framework/index.js';

const DEV = TAGS.DEV;
const CHECKIN = TAGS.CHECKIN;

e2eTest('two players are dealt a hand and each sees only their own hole cards', DEV, async (t) => {
  const server = await launchServer(t);
  const alice = await createPlayer(t, server, 'alice');
  const bob = await createPlayer(t, server, 'bob');

  await alice.joinAndSit(500);
  await bob.joinAndSit(500);

  await alice.client.waitFor(
    (events) => events.filter((event) => event.event === 'sit').length === 2,
    { description: 'both sit events' },
  );
  const seats = alice.client.events
    .filter((event) => event.event === 'sit')
    .map((event) => event.seat);
  assert.deepEqual(seats, [0, 1]);

  await server.tick();

  // Hole cards are pushed before the public deal event on each connection.
  const aliceCards = await alice.client.waitForEvent('holeCards');
  const bobCards = await bob.client.waitForEvent('holeCards');
  assert.equal(aliceCards.seat, 0);
  assert.equal(bobCards.seat, 1);
  for (const card of [...aliceCards.cards, ...bobCards.cards]) {
    assert.ok(card.rank, `card should have a rank: ${JSON.stringify(card)}`);
    assert.ok(card.suit, `card should have a suit: ${JSON.stringify(card)}`);
  }

  assert.deepEqual((await alice.client.waitForEvent('deal')).seatsDealt, [0, 1]);
  assert.deepEqual((await bob.client.waitForEvent('deal')).seatsDealt, [0, 1]);

  // Hole cards are private: no duplicate and no opponent card.
  assert.equal(alice.client.events.filter((event) => event.event === 'holeCards').length, 1);
  assert.equal(bob.client.events.filter((event) => event.event === 'holeCards').length, 1);
});

e2eTest('a lone player is not dealt until a second player sits', DEV, async (t) => {
  const server = await launchServer(t);
  const alice = await createPlayer(t, server, 'alice');
  await alice.joinAndSit(500);
  await alice.client.waitForEvent('sit');

  await server.tick();
  await alice.client.expectNoEvent('deal', { withinMs: 150 });

  const bob = await createPlayer(t, server, 'bob');
  await bob.joinAndSit(500);
  await alice.client.waitFor(
    (events) => events.filter((event) => event.event === 'sit').length === 2,
    { description: 'the second sit event' },
  );

  await server.tick();

  assert.deepEqual((await alice.client.waitForEvent('deal')).seatsDealt, [0, 1]);
});

e2eTest('a spectator sees table events but no hole cards', DEV, async (t) => {
  const server = await launchServer(t);
  const spectator = await createPlayer(t, server, 'spectator');
  await spectator.joinTable();

  const alice = await createPlayer(t, server, 'alice');
  await alice.joinAndSit(500);

  const sit = await spectator.client.waitForEvent('sit');
  assert.equal(sit.seat, 0);

  await server.tick();
  await spectator.client.expectNoEvent('holeCards', { withinMs: 150 });
});

e2eTest('leaveTable, stand, bet and fold are acknowledged', CHECKIN, async (t) => {
  const server = await launchServer(t);
  const alice = await createPlayer(t, server, 'alice');
  const bob = await createPlayer(t, server, 'bob');
  await alice.joinAndSit(500);
  await bob.joinAndSit(500);
  await alice.client.waitFor(
    (events) => events.filter((event) => event.event === 'sit').length === 2,
    { description: 'both sit events' },
  );

  assert.equal((await alice.client.leaveTable()).command, 'leaveTable');
  assert.equal((await alice.client.stand()).response, 'success');
  assert.equal((await alice.client.bet(50)).response, 'success');
  assert.equal((await alice.client.fold()).response, 'success');

  // None of those actions has reached the core yet, so the hand still deals.
  await server.tick();
  assert.deepEqual((await alice.client.waitForEvent('deal')).seatsDealt, [0, 1]);
});

e2eTest('a table action is accepted for a table id the server does not know', CHECKIN, async (t) => {
  const server = await launchServer(t);
  const alice = await createPlayer(t, server, 'alice');
  await alice.joinTable();

  // The server ignores tableId today (README §3.2), so a random UUID is fine.
  const response = await alice.client.sit(500, crypto.randomUUID());

  assert.equal(response.response, 'success');
});
