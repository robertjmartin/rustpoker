import { Show, createSignal } from "solid-js";

import { seatLabel } from "../lib/protocol";

interface ActionBarProps {
  seated: boolean;
  seat: number | null;
  stack: number | null;
  turn: boolean;
  acting: number | null;
  inHand: boolean;
  toCall: number;
  onSit: (amount: number) => void;
  onStand: () => void;
  onBet: (amount: number) => void;
  onCheck: () => void;
  onFold: () => void;
}

export function ActionBar(props: ActionBarProps) {
  const [sitAmount, setSitAmount] = createSignal(500);
  const [betAmount, setBetAmount] = createSignal(50);

  const status = () => {
    if (props.turn) return "Your turn";
    if (props.acting !== null) return `${seatLabel(props.acting)} is acting`;
    return props.inHand ? "Waiting" : "Waiting for the next hand";
  };

  return (
    <div class="actions">
      <Show
        when={props.seated}
        fallback={
          <>
            <div class="group">
              <label class="amount">
                <span>Chips</span>
                <input
                  type="number"
                  min="1"
                  step="50"
                  value={sitAmount()}
                  onInput={(event) => {
                    const value = Math.max(1, Math.floor(Number(event.currentTarget.value) || 0));
                    setSitAmount(value);
                  }}
                />
              </label>
              <button class="primary" type="button" onClick={() => props.onSit(sitAmount())}>
                Take a seat
              </button>
            </div>
            <p class="hint">
              You take the first open seat. A hand starts once at least two players are seated.
            </p>
          </>
        }
      >
        <div class="group">
          <span class="badge">{props.seat !== null ? seatLabel(props.seat) : ""}</span>
          <span class="badge soft">{props.stack ?? "—"} chips</span>
          <span class={`turn-badge ${props.turn ? "on" : ""}`}>{status()}</span>
        </div>

        <div class="spacer" />

        <div class="group">
          <button type="button" disabled={!props.turn || props.toCall > 0} onClick={props.onCheck}>
            Check
          </button>
          <button type="button" disabled={!props.turn || props.toCall === 0} onClick={() => props.onBet(props.toCall)}>
            {props.toCall > 0 ? `Call ${props.toCall}` : "Call"}
          </button>

          <label class="amount">
            <span>Bet</span>
            <input
              type="number"
              min="1"
              step="10"
              value={betAmount()}
              onInput={(event) => {
                const value = Math.max(1, Math.floor(Number(event.currentTarget.value) || 0));
                setBetAmount(value);
              }}
            />
          </label>
          <button type="button" disabled={!props.turn || betAmount() < 1} onClick={() => props.onBet(betAmount())}>
            Bet
          </button>
          <button type="button" class="danger" disabled={!props.turn} onClick={props.onFold}>
            Fold
          </button>
          <button type="button" class="ghost" onClick={props.onStand}>
            Stand
          </button>
        </div>

        <p class="hint">Check is only legal with nothing to call; Call bets exactly the difference.</p>
      </Show>
    </div>
  );
}
