import { Show, createSignal } from "solid-js";

export function Lobby(props: {
  busy: boolean;
  notice: string;
  onRegister: (name: string) => void;
  onResume: (userId: string) => void;
}) {
  const [name, setName] = createSignal("");
  const [userId, setUserId] = createSignal("");

  return (
    <section class="lobby">
      <div class="lobby-card">
        <h1>
          <span class="pip">♠</span> rustpoker
        </h1>
        <p class="lede">
          No account, no password. Pick a name, get 1,000 chips and take a seat at the table.
        </p>

        <form
          onSubmit={(event) => {
            event.preventDefault();
            const trimmed = name().trim();
            if (trimmed) props.onRegister(trimmed);
          }}
        >
          <div class="field">
            <label for="lobby-name">Player name</label>
            <input
              id="lobby-name"
              value={name()}
              onInput={(event) => setName(event.currentTarget.value)}
              placeholder="alice"
              autocomplete="off"
              maxlength="24"
            />
          </div>
          <button class="primary wide" type="submit" disabled={props.busy || !name().trim()}>
            Create player
          </button>
        </form>

        <Show when={props.notice}>
          <p class="notice">{props.notice}</p>
        </Show>

        <details class="resume">
          <summary>Resume with an existing user id</summary>
          <div class="field">
            <label for="lobby-id">User id</label>
            <input
              id="lobby-id"
              value={userId()}
              onInput={(event) => setUserId(event.currentTarget.value)}
              placeholder="uuid returned by /register"
              autocomplete="off"
              spellcheck={false}
            />
          </div>
          <button
            type="button"
            disabled={props.busy || !userId().trim()}
            onClick={() => props.onResume(userId().trim())}
          >
            Resume
          </button>
        </details>

        <p class="lobby-foot">
          The server keeps players in memory, so restarting it clears every user and table.
        </p>
      </div>
    </section>
  );
}
