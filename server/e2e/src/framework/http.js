// Thin wrappers over the server's HTTP surface. Tests use these instead of
// fetch directly, so a protocol change is a one-line change here.

export class HttpAssertionError extends Error {}

export async function requestJson(baseUrl, method, route, body) {
  const response = await fetch(baseUrl + route, {
    method,
    headers: body === undefined ? undefined : { 'content-type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await response.text();
  let parsed = null;
  try {
    parsed = text ? JSON.parse(text) : null;
  } catch {
    parsed = text;
  }
  return { status: response.status, body: parsed, text };
}

function expectStatus({ status, text }, expected, description) {
  if (status !== expected) {
    throw new HttpAssertionError(`${description}: expected HTTP ${expected}, got ${status} (${text})`);
  }
}

export async function ping(server) {
  const result = await requestJson(server.httpUrl, 'GET', '/ping');
  expectStatus(result, 200, 'GET /ping');
  return result.body;
}

export async function registerUser(server, name) {
  const result = await requestJson(server.httpUrl, 'POST', '/register', { name });
  expectStatus(result, 201, `POST /register (${name})`);
  return result.body;
}

export async function getUser(server, id) {
  return requestJson(server.httpUrl, 'GET', `/user/${encodeURIComponent(id)}`);
}

export async function getUserCount(server) {
  const result = await requestJson(server.httpUrl, 'GET', '/usercount');
  expectStatus(result, 200, 'GET /usercount');
  return result.body.count;
}
