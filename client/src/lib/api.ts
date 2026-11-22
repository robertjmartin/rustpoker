import type { User } from "./protocol";

// Empty in development: Vite proxies the endpoints to pokerserver.
const base = (import.meta.env.VITE_SERVER_URL as string | undefined) ?? "";

export async function registerUser(name: string): Promise<User> {
  const response = await fetch(`${base}/register`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name }),
  });
  if (!response.ok) throw new Error(`register failed (${response.status})`);
  return (await response.json()) as User;
}

/** `null` when the server does not know the id (for example after a restart). */
export async function fetchUser(id: string): Promise<User | null> {
  const response = await fetch(`${base}/user/${encodeURIComponent(id)}`);
  if (response.status === 404) return null;
  if (!response.ok) throw new Error(`user lookup failed (${response.status})`);
  return (await response.json()) as User;
}
