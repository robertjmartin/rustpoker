// Area: the HTTP surface every client depends on.
// Buckets come from tags: every test is in `full`, these are also in `quick`.
import assert from 'node:assert/strict';
import { TAGS, e2eTest } from '../framework/test.js';
import {
  getUser,
  getUserCount,
  launchServer,
  ping,
  registerUser,
} from '../framework/index.js';

const TAG = TAGS.DEV;

e2eTest('GET /ping reports ok', TAG, async (t) => {
  const server = await launchServer(t);

  const body = await ping(server);

  assert.deepEqual(body, { status: 'ok' });
});

e2eTest('POST /register creates a user with a starting balance', TAG, async (t) => {
  const server = await launchServer(t);

  const user = await registerUser(server, 'alice');

  assert.match(
    user.id,
    /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i,
    'id should be a v4 UUID',
  );
  assert.equal(user.name, 'alice');
  assert.equal(user.balance, 1000);
});

e2eTest('GET /user/{id} returns the registered user', TAG, async (t) => {
  const server = await launchServer(t);
  const user = await registerUser(server, 'bob');

  const result = await getUser(server, user.id);

  assert.equal(result.status, 200);
  assert.deepEqual(result.body, user);
});

e2eTest('GET /user/{id} returns 404 for an unknown id', TAG, async (t) => {
  const server = await launchServer(t);

  const result = await getUser(server, crypto.randomUUID());

  assert.equal(result.status, 404);
});

e2eTest('GET /usercount counts registered users', TAG, async (t) => {
  const server = await launchServer(t);
  assert.equal(await getUserCount(server), 0);

  await registerUser(server, 'alice');
  await registerUser(server, 'bob');

  assert.equal(await getUserCount(server), 2);
});
