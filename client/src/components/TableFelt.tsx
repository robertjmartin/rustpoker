import { For, Show } from "solid-js";

import type { GameState } from "../lib/game";
import { seatLabel, type Card } from "../lib/protocol";
import { PlayingCard } from "./PlayingCard";

export function TableFelt(props: { state: GameState }) {
  const seatStatus = (seat: GameState["seats"][number]) => {
    if (!seat.occupied) return "open";
    if (seat.folded) return "folded";
    return seat.lastAction ?? "in hand";
  };

  // Your own hand is private until the hand ends; everyone else's cards only
  // become visible when the server publishes them at showdown.
  const visibleCards = (index: number): Card[] | null => {
    if (index === props.state.mySeat) return props.state.myCards;
    return props.state.showdown.find((hand) => hand.seat === index)?.cards ?? null;
  };

  return (
    <div class="felt">
      <div class="felt-line" aria-hidden="true" />

      <div class="felt-center">
        <Show when={props.state.result}>
          {(result) => (
            <div class="result">
              <strong>{result().winners.map(seatLabel).join(", ")}</strong>
              <span>
                {result().winners.length === 1 ? "wins" : "split"} {result().amount}
              </span>
              <span class="result-tag">{result().showdown ? "showdown" : "all folded"}</span>
            </div>
          )}
        </Show>

        <div class="board" aria-label="Community cards">
          <For each={[0, 1, 2, 3, 4]}>{(index) => <PlayingCard card={props.state.board[index]} />}</For>
        </div>

        <div class="pot">
          <span class="pot-label">Pot</span>
          <span class="pot-value">{props.state.pot}</span>
        </div>
      </div>

      <For each={props.state.seats}>
        {(seat) => (
          <div
            class={[
              "seat",
              `seat-${seat.index}`,
              seat.occupied ? "occupied" : "empty",
              seat.index === props.state.acting ? "acting" : "",
              seat.index === props.state.mySeat ? "mine" : "",
              seat.folded ? "folded" : "",
            ]
              .filter(Boolean)
              .join(" ")}
          >
            <Show when={visibleCards(seat.index)}>
              {(cards) => (
                <div class="seat-cards">
                  <For each={cards()}>{(card) => <PlayingCard card={card} size="sm" />}</For>
                </div>
              )}
            </Show>

            <div class="seat-name">{seat.index === props.state.mySeat ? "You" : seatLabel(seat.index)}</div>

            <div class="seat-meta">
              <Show
                when={seat.bet > 0}
                fallback={<span class="seat-tag">{seatStatus(seat)}</span>}
              >
                <span class="bet-chip">{seat.bet}</span>
              </Show>
            </div>
          </div>
        )}
      </For>
    </div>
  );
}
