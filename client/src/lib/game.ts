import { createStore, produce } from "solid-js/store";

import { SEAT_COUNT, cardLabel, seatLabel, type Card, type GameEvent, type ShowdownHand } from "./protocol";

export interface SeatState {
  index: number;
  occupied: boolean;
  /** Folded, or sitting out the hand that is currently running. */
  folded: boolean;
  /** Chips put in during the current betting round. */
  bet: number;
  lastAction: string | null;
}

export interface HandResult {
  winners: number[];
  amount: number;
  showdown: boolean;
}

export interface GameState {
  seats: SeatState[];
  board: Card[];
  pot: number;
  /** Table stakes, from the snapshot; 0 until the first snapshot arrives. */
  smallBlind: number;
  bigBlind: number;
  ante: number;
  /** Seat the server says is to act. */
  acting: number | null;
  mySeat: number | null;
  myCards: Card[] | null;
  /** Tracked locally: the server only publishes bets, not stacks. */
  myStack: number | null;
  inHand: boolean;
  result: HandResult | null;
  /** Hands published at the last showdown, cleared when the next hand starts. */
  showdown: ShowdownHand[];
}

function emptySeats(): SeatState[] {
  return Array.from({ length: SEAT_COUNT }, (_, index) => ({
    index,
    occupied: false,
    folded: false,
    bet: 0,
    lastAction: null,
  }));
}

function initialState(): GameState {
  return {
    seats: emptySeats(),
    board: [],
    pot: 0,
    smallBlind: 0,
    bigBlind: 0,
    ante: 0,
    acting: null,
    mySeat: null,
    myCards: null,
    myStack: null,
    inHand: false,
    result: null,
    showdown: [],
  };
}

/**
 * Table state built from the server's snapshot on join plus the event stream.
 * `apply` folds one frame in and returns a line for the log.
 */
export function createGame() {
  const [state, setState] = createStore<GameState>(initialState());

  // The sit event carries no user, so remember the stack we asked to sit with;
  // the next sit event we see after it is ours.
  let pendingSit: number | null = null;

  function apply(event: GameEvent): string | null {
    switch (event.event) {
      case "sit": {
        const amount = pendingSit;
        const mine = amount !== null;
        if (mine) pendingSit = null;
        setState(
          produce((s) => {
            const seat = s.seats[event.seat];
            if (!seat) return;
            seat.occupied = true;
            seat.folded = false;
            seat.bet = 0;
            seat.lastAction = null;
            if (mine) {
              s.mySeat = event.seat;
              s.myStack = amount;
            }
          }),
        );
        return `${seatLabel(event.seat)} sat down${mine ? " (you)" : ""}`;
      }

      case "leave": {
        setState(
          produce((s) => {
            const seat = s.seats[event.seat];
            if (!seat) return;
            seat.occupied = false;
            seat.folded = false;
            seat.bet = 0;
            seat.lastAction = null;
            if (s.mySeat === event.seat) {
              s.mySeat = null;
              s.myCards = null;
              s.myStack = null;
            }
          }),
        );
        return `${seatLabel(event.seat)} left the table`;
      }

      case "deal": {
        const dealt = new Set(event.seatsDealt);
        setState(
          produce((s) => {
            s.board = [];
            s.pot = 0;
            s.acting = null;
            s.inHand = true;
            s.result = null;
            s.showdown = [];
            s.myCards = null;
            for (const seat of s.seats) {
              seat.bet = 0;
              seat.lastAction = null;
              seat.folded = seat.occupied && !dealt.has(seat.index);
            }
          }),
        );
        return `New hand — dealt to ${event.seatsDealt.map(seatLabel).join(", ")}`;
      }

      case "ante": {
        setState(
          produce((s) => {
            const seat = s.seats[event.seat];
            if (!seat) return;
            // An ante is dead money: it goes straight to the pot and is not a
            // street bet, so it never counts toward the call.
            seat.lastAction = `ante ${event.amount}`;
            s.pot += event.amount;
            if (s.mySeat === event.seat && s.myStack !== null) {
              s.myStack = Math.max(0, s.myStack - event.amount);
            }
          }),
        );
        return `${seatLabel(event.seat)} antes ${event.amount}`;
      }

      case "blind": {
        setState(
          produce((s) => {
            const seat = s.seats[event.seat];
            if (!seat) return;
            // A blind is live: it is part of the current street's bet.
            seat.bet += event.amount;
            seat.lastAction = `${event.kind} blind ${event.amount}`;
            s.pot += event.amount;
            if (s.mySeat === event.seat && s.myStack !== null) {
              s.myStack = Math.max(0, s.myStack - event.amount);
            }
          }),
        );
        return `${seatLabel(event.seat)} posts the ${event.kind} blind ${event.amount}`;
      }

      case "bet": {
        setState(
          produce((s) => {
            const seat = s.seats[event.seat];
            if (!seat) return;
            seat.bet += event.amount;
            seat.lastAction = `bet ${event.amount}`;
            s.pot += event.amount;
            if (s.mySeat === event.seat && s.myStack !== null) {
              s.myStack = Math.max(0, s.myStack - event.amount);
            }
          }),
        );
        return `${seatLabel(event.seat)} bets ${event.amount}`;
      }

      case "check": {
        setState(
          produce((s) => {
            const seat = s.seats[event.seat];
            if (seat) seat.lastAction = "checked";
          }),
        );
        return `${seatLabel(event.seat)} checks`;
      }

      case "fold": {
        setState(
          produce((s) => {
            const seat = s.seats[event.seat];
            if (!seat) return;
            seat.folded = true;
            seat.lastAction = "folded";
          }),
        );
        return `${seatLabel(event.seat)} folds`;
      }

      case "turn": {
        setState("acting", event.seat);
        return `${seatLabel(event.seat)} to act`;
      }

      case "communityCards": {
        setState(
          produce((s) => {
            s.board = [...s.board, ...event.cards];
            // Street contributions have moved into the pot.
            for (const seat of s.seats) {
              seat.bet = 0;
              seat.lastAction = null;
            }
          }),
        );
        const street = state.board.length === 3 ? "Flop" : state.board.length === 4 ? "Turn" : "River";
        return `${street}: ${event.cards.map(cardLabel).join(" ")}`;
      }

      case "handEnd": {
        setState(
          produce((s) => {
            s.pot = 0;
            s.acting = null;
            s.inHand = false;
            s.result = { winners: event.winners, amount: event.amount, showdown: event.showdown };
            if (s.mySeat !== null && event.winners.includes(s.mySeat) && s.myStack !== null) {
              s.myStack += Math.floor(event.amount / event.winners.length);
            }
          }),
        );
        const winners = event.winners.map(seatLabel).join(", ");
        const verb = event.winners.length === 1 ? "wins" : "split";
        return `${winners} ${verb} ${event.amount}${event.showdown ? " at showdown" : " (all folded)"}`;
      }

      case "holeCards": {
        setState(
          produce((s) => {
            s.mySeat = event.seat;
            s.myCards = event.cards;
          }),
        );
        return `Your hand: ${event.cards.map(cardLabel).join(" ")}`;
      }

      case "showdown": {
        setState(
          produce((s) => {
            s.showdown = event.hands.map((hand) => ({ seat: hand.seat, cards: [...hand.cards] }));
          }),
        );
        if (event.hands.length === 0) return null;
        const hands = event.hands
          .map((hand) => `${seatLabel(hand.seat)} ${hand.cards.map(cardLabel).join(" ")}`)
          .join(", ");
        return `Showdown: ${hands}`;
      }

      case "snapshot": {
        // The snapshot is authoritative: replace everything derived from events.
        setState(
          produce((s) => {
            s.seats = event.seats
              .slice()
              .sort((a, b) => a.seat - b.seat)
              .map((seat) => ({
                index: seat.seat,
                occupied: seat.occupied,
                folded: seat.folded,
                bet: seat.bet,
                lastAction: null,
              }));
            s.board = [...event.board];
            s.pot = event.pot;
            s.smallBlind = event.smallBlind;
            s.bigBlind = event.bigBlind;
            s.ante = event.ante;
            s.acting = event.street === "idle" ? null : event.acting;
            s.inHand = event.street !== "idle";
            s.result = null;
            s.showdown = [];
            s.mySeat = event.yourSeat;
            s.myCards = event.yourCards ? [...event.yourCards] : null;
            const mine =
              event.yourSeat === null ? undefined : event.seats.find((seat) => seat.seat === event.yourSeat);
            s.myStack = mine ? mine.stack : null;
          }),
        );
        return null;
      }

      default:
        return null;
    }
  }

  return {
    state,
    apply,
    reset() {
      pendingSit = null;
      setState(initialState());
    },
    noteSitRequested(amount: number) {
      pendingSit = amount;
    },
    cancelPendingSit() {
      pendingSit = null;
    },
  };
}

export type Game = ReturnType<typeof createGame>;
