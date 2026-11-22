import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import { viteSingleFile } from "vite-plugin-singlefile";

// Kept local so the client does not need @types/node just for one env read.
declare const process: { env: Record<string, string | undefined> };

// Where pokerserver listens. Override with POKER_SERVER=http://host:port.
const backend = process.env.POKER_SERVER ?? "http://localhost:3000";

export default defineConfig({
  // The production build is one self-contained HTML file. pokerserver embeds
  // it with include_str!, so it must not reference external /assets URLs.
  plugins: [solid(), viteSingleFile()],
  server: {
    port: 5173,
    proxy: {
      "/ws": { target: backend, ws: true },
      "/ping": { target: backend },
      "/register": { target: backend },
      "/user": { target: backend },
      "/usercount": { target: backend },
    },
  },
});
