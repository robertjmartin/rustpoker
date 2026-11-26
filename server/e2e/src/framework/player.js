// High-level player: register over HTTP, open one socket, authenticate, and
// offer the usual table actions. Scenarios should read like a user story
// ("alice joins and sits") rather than like protocol plumbing.
import { registerUser } from './http.js';
import { TestClient } from './client.js';

export async function createPlayer(t, server, name, { bankAmount = 500 } = {}) {
  const user = await registerUser(server, name);
  const client = await TestClient.connect(server, { label: name });
  t.after(() => client.close());

  const auth = await client.setAuthentication(user.id);
  if (auth.response !== 'success') {
    throw new Error(`failed to authenticate ${name}: ${JSON.stringify(auth)}`);
  }

  return {
    name,
    user,
    client,

    /** Authenticate this player's connection to the table (subscribe to events). */
    async joinTable(tableId) {
      const response = await client.joinTable(tableId);
      if (response.response !== 'success') {
        throw new Error(`joinTable failed for ${name}: ${JSON.stringify(response)}`);
      }
      return response;
    },

    /**
     * Join the table and sit down. The server does not tell a player their
     * seat; tests observe the broadcast `sit` events to learn seat numbers.
     */
    async joinAndSit(amount = bankAmount) {
      await this.joinTable();
      const response = await client.sit(amount);
      if (response.response !== 'success') {
        throw new Error(`sit failed for ${name}: ${JSON.stringify(response)}`);
      }
      return response;
    },
  };
}
