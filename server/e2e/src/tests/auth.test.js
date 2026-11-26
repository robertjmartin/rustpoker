// Area: the WebSocket authentication handshake and its error paths.
import assert from 'node:assert/strict';
import { TAGS, e2eTest } from '../framework/test.js';
import { TestClient, createPlayer, launchServer } from '../framework/index.js';

const TAG = TAGS.DEV;

e2eTest('setAuthentication binds the connection to an existing user', TAG, async (t) => {
  const server = await launchServer(t);
  const alice = await createPlayer(t, server, 'alice');

  const response = await alice.client.getAuthentication();

  assert.equal(response.response, 'success');
  // Current server behaviour: getAuthentication reports the command name of
  // setAuthentication (documented in README §3.2).
  assert.equal(response.command, 'setAuthentication');
  assert.equal(response.userId, alice.user.id);
});

e2eTest('setAuthentication rejects an unknown user id', TAG, async (t) => {
  const server = await launchServer(t);
  const client = await TestClient.connect(server);
  t.after(() => client.close());

  const response = await client.setAuthentication(crypto.randomUUID());

  assert.equal(response.response, 'error');
  assert.equal(response.error, 'unknown');

  // A failed attempt must not leave the connection authenticated.
  const join = await client.joinTable();
  assert.equal(join.response, 'error');
  assert.equal(join.error, 'notAuthenticated');
});

e2eTest('table commands require authentication', TAG, async (t) => {
  const server = await launchServer(t);
  const client = await TestClient.connect(server);
  t.after(() => client.close());

  const join = await client.joinTable();
  assert.equal(join.error, 'notAuthenticated');

  const sit = await client.sit(500);
  assert.equal(sit.error, 'notAuthenticated');
});

e2eTest('leaveTable is acknowledged without authentication', TAG, async (t) => {
  const server = await launchServer(t);
  const client = await TestClient.connect(server);
  t.after(() => client.close());

  const response = await client.leaveTable();

  assert.equal(response.response, 'success');
  assert.equal(response.command, 'leaveTable');
});
