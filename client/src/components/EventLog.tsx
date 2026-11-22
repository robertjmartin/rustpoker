import { For, Show, createEffect } from "solid-js";

export interface LogEntry {
  id: number;
  kind: "event" | "system" | "mine" | "error";
  text: string;
  time: string;
}

export function EventLog(props: { entries: LogEntry[]; onClear: () => void }) {
  let list: HTMLOListElement | undefined;

  createEffect(() => {
    // Track the length so the list follows new lines.
    props.entries.length;
    if (list) list.scrollTop = list.scrollHeight;
  });

  return (
    <aside class="log">
      <div class="log-head">
        <span>Hand log</span>
        <button type="button" onClick={props.onClear}>
          Clear
        </button>
      </div>
      <ol class="log-list" ref={list}>
        <Show when={props.entries.length === 0}>
          <li class="log-empty">Nothing yet. Table events will appear here.</li>
        </Show>
        <For each={props.entries}>
          {(entry) => (
            <li class={`log-item ${entry.kind}`}>
              <span class="time">{entry.time}</span>
              <span class="text">{entry.text}</span>
            </li>
          )}
        </For>
      </ol>
    </aside>
  );
}
