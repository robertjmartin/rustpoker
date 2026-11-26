// Area: disconnects. The server has no cleanup for dropped connections yet,
// so a disconnect must at least not disturb the remaining players or block new
// ones.
import assert from 'node:assert/strict';
import { TAGS, e2eTest } from '../framework/test.js';
import { createPlayer, launchServer } from '../framework/index.js';

const TAG = TAGS.CHECKIN;

e2eTest('a player disconnecting does not stop the table for the rest', TAG, async (t) => {
  const server = await launchServer(t);
  const alice = await createPlayer(t, server, 'alice');
  const bob = await createPlayer(t, server, 'bob');
  await alice.joinAndSit(500);
  await bob.joinAndSit(500);
  await alice.client.waitFor(
    (events) => events.filter((event) => event.event === 'sit').length === 2,
    { description: 'both sit events' },
  );

  await bob.client.close();
  await server.tick();

  const holeCards = await alice.client.waitForEvent('holeCards');
  assert.equal(holeCards.seat, 0);
  // The abandoned seat still holds chips, so it stays in the hand.
  assert.deepEqual((await alice.client.waitForEvent('deal')).seatsDealt, [0, 1]);
});

e2eTest('a new player can sit after another disconnects', TAG, async (t) => {
  const server = await launchServer(t);
  const alice = await createPlayer(t, server, 'alice');
  const bob = await createPlayer(t, server, 'bob');
  await alice.joinAndSit(500);
  await bob.joinAndSit(500);
  await alice.client.waitFor(
    (events) => events.filter((event) => event.event === 'sit').length === 2,
    { description: 'both sit events' },
  );
  await bob.client.close();

  const charlie = await createPlayer(t, server, 'charlie');
  await charlie.joinAndSit(500);

  const sit = await alice.client.waitForEvent('sit', { where: (event) => event.seat === 2 });
  assert.equal(sit.seat, 2, 'the disconnected seat is not freed yet, so seat 2 is next');
});
