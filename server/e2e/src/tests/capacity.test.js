// Area: table capacity. Eight seats deal; the ninth player is seated nowhere
// even though the connection layer acknowledges the command.
import assert from 'node:assert/strict';
import { TAGS, e2eTest } from '../framework/test.js';
import { createPlayer, launchServer } from '../framework/index.js';

const TAG = TAGS.CHECKIN;

e2eTest('eight players fill the table and a ninth is not seated', TAG, async (t) => {
  const server = await launchServer(t);
  const players = [];
  for (let i = 0; i < 8; i++) {
    players.push(await createPlayer(t, server, `player${i}`));
  }

  for (const player of players) {
    await player.joinAndSit(1000);
  }

  const watcher = players[0].client;
  await watcher.waitFor(
    (events) => events.filter((event) => event.event === 'sit').length === 8,
    { description: 'eight sit events' },
  );
  const seats = watcher.events
    .filter((event) => event.event === 'sit')
    .map((event) => event.seat)
    .sort((a, b) => a - b);
  assert.deepEqual(seats, [0, 1, 2, 3, 4, 5, 6, 7]);

  // Connect the ninth player only now, so its event stream has no old sits.
  const ninth = await createPlayer(t, server, 'player8');
  await ninth.client.joinTable();
  const sit = await ninth.client.sit(1000);
  assert.equal(sit.response, 'success', 'the command is queued, not rejected');
  await ninth.client.expectNoEvent('sit', { withinMs: 300 });

  await server.tick();

  const dealtSeats = [];
  for (const player of players) {
    const holeCards = await player.client.waitForEvent('holeCards');
    assert.equal(holeCards.cards.length, 2);
    dealtSeats.push(holeCards.seat);
  }
  assert.deepEqual(dealtSeats.sort((a, b) => a - b), [0, 1, 2, 3, 4, 5, 6, 7]);

  assert.deepEqual((await watcher.waitForEvent('deal')).seatsDealt, [0, 1, 2, 3, 4, 5, 6, 7]);
  await ninth.client.expectNoEvent('holeCards', { withinMs: 200 });
}, {trace: true});
