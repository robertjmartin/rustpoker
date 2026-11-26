// Area: the isolation guarantee the framework relies on. Two servers started
// together get different ports and separate in-memory state.
import assert from 'node:assert/strict';
import { TAGS, e2eTest } from '../framework/test.js';
import {
  TestClient,
  getUserCount,
  launchServer,
  registerUser,
} from '../framework/index.js';

const TAG = TAGS.CHECKIN;

e2eTest('servers launched in parallel have isolated ports and state', TAG, async (t) => {
  const [first, second] = await Promise.all([launchServer(t), launchServer(t)]);
  assert.notEqual(first.port, second.port);

  const alice = await registerUser(first, 'alice');
  assert.equal(await getUserCount(first), 1);
  assert.equal(await getUserCount(second), 0);

  // A user from the first server is unknown to the second.
  const client = await TestClient.connect(second);
  t.after(() => client.close());
  const auth = await client.setAuthentication(alice.id);
  assert.equal(auth.response, 'error');
  assert.equal(auth.error, 'unknown');
});
