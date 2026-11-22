import { Show } from "solid-js";

import { RANK_LABEL, SUIT_GLYPH, isRed, type Card } from "../lib/protocol";

export function PlayingCard(props: { card?: Card; size?: "sm" }) {
  return (
    <Show when={props.card} fallback={<span class="card placeholder" aria-hidden="true" />}>
      {(card) => (
        <span
          class={`card ${isRed(card().suit) ? "red" : "black"} ${props.size ?? ""}`.trim()}
          title={`${RANK_LABEL[card().rank]} of ${card().suit}`}
        >
          <span class="rank">{RANK_LABEL[card().rank]}</span>
          <span class="suit">{SUIT_GLYPH[card().suit]}</span>
        </span>
      )}
    </Show>
  );
}
