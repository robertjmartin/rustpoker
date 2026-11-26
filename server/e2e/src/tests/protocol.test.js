// Area: envelope-level robustness. Bad input is ignored server-side (no
// response) and must not take the connection down.
import assert from 'node:assert/strict';
import { TAGS, e2eTest } from '../framework/test.js';
import { createPlayer, launchServer, requestJson } from '../framework/index.js';

const TAG = TAGS.CHECKIN;

e2eTest('a malformed frame is ignored and the connection stays usable', TAG, async (t) => {
  const server = await launchServer(t);
  const alice = await createPlayer(t, server, 'alice');

  alice.client.sendRaw('{not json');
  await alice.client.expectNoResponse({ withinMs: 300 });

  const response = await alice.client.getAuthentication();
  assert.equal(response.response, 'success');
  assert.equal(response.userId, alice.user.id);
});

e2eTest('an unknown command is ignored', TAG, async (t) => {
  const server = await launchServer(t);
  const alice = await createPlayer(t, server, 'alice');

  alice.client.sendRaw(JSON.stringify({ id: 99, command: 'launchMissiles' }));
  await alice.client.expectNoResponse({ withinMs: 300 });

  const join = await alice.client.joinTable();
  assert.equal(join.response, 'success');
});

e2eTest('the test tick endpoint only exists when test hooks are enabled', TAG, async (t) => {
  const server = await launchServer(t, { testHooks: false });

  const result = await requestJson(server.httpUrl, 'POST', '/test/tick');

  assert.equal(result.status, 404);
});

e2eTest('a table id that is not a UUID fails parsing', TAG, async (t) => {
  const server = await launchServer(t);
  const alice = await createPlayer(t, server, 'alice');

  alice.client.sendRaw(JSON.stringify({ id: 1, command: 'joinTable', tableId: 'not-a-uuid' }));
  await alice.client.expectNoResponse({ withinMs: 300 });

  const join = await alice.client.joinTable();
  assert.equal(join.response, 'success');
});
