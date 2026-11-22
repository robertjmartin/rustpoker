/** Number of seats at the server's single table. */
export const SEAT_COUNT = 8;

/** Sent with every table command. The server parses it but ignores it today. */
export const TABLE_ID = "00000000-0000-0000-0000-000000000000";

export type Suit = "hearts" | "diamonds" | "spades" | "clubs";

export type Rank =
  | "two"
  | "three"
  | "four"
  | "five"
  | "six"
  | "seven"
  | "eight"
  | "nine"
  | "ten"
  | "jack"
  | "queen"
  | "king"
  | "ace";

export interface Card {
  suit: Suit;
  rank: Rank;
}

/** `GET /user/{id}`, `POST /register`. */
export interface User {
  id: string;
  name: string;
  balance: number;
}

export type TableAction =
  | { action: "sit"; bankAmount: number }
  | { action: "stand" }
  | { action: "bet"; betAmount: number }
  | { action: "check" }
  | { action: "fold" };

/** A client request. The socket adds the numeric `id` before sending. */
export type ClientCommand =
  | { command: "setAuthentication"; userId: string }
  | { command: "getAuthentication" }
  | { command: "getHoleCards" }
  | { command: "joinTable"; tableId: string }
  | { command: "leaveTable" }
  | { command: "table"; tableId: string; action: TableAction };

export interface ServerResponse {
  id: number;
  response: "success" | "error";
  command?: string;
  error?: string;
  userId?: string;
}

export type StreetName = "idle" | "preFlop" | "flop" | "turn" | "river";

export interface SeatSnapshot {
  seat: number;
  occupied: boolean;
  stack: number;
  bet: number;
  folded: boolean;
}

/** Private frame pushed after `joinTable`, describing the whole table. */
export interface TableSnapshot {
  seats: SeatSnapshot[];
  board: Card[];
  pot: number;
  currentBet: number;
  smallBlind: number;
  bigBlind: number;
  ante: number;
  acting: number | null;
  dealer: number | null;
  street: StreetName;
  yourSeat: number | null;
  yourCards: [Card, Card] | null;
}

export type BlindKind = "small" | "big";

export interface ShowdownHand {
  seat: number;
  cards: Card[];
}

/** Public events (broadcast), the private `holeCards` frame and the snapshot. */
export type GameEvent =
  | { event: "sit"; seat: number }
  | { event: "leave"; seat: number }
  | { event: "deal"; seatsDealt: number[] }
  | { event: "ante"; seat: number; amount: number }
  | { event: "blind"; seat: number; amount: number; kind: BlindKind }
  | { event: "bet"; seat: number; amount: number }
  | { event: "check"; seat: number }
  | { event: "fold"; seat: number }
  | { event: "turn"; seat: number }
  | { event: "communityCards"; cards: Card[] }
  | { event: "showdown"; hands: ShowdownHand[] }
  | { event: "handEnd"; winners: number[]; amount: number; showdown: boolean }
  | { event: "holeCards"; seat: number; cards: Card[] }
  | ({ event: "snapshot" } & TableSnapshot);

export const SUIT_GLYPH: Record<Suit, string> = {
  spades: "♠",
  hearts: "♥",
  diamonds: "♦",
  clubs: "♣",
};

export const RANK_LABEL: Record<Rank, string> = {
  two: "2",
  three: "3",
  four: "4",
  five: "5",
  six: "6",
  seven: "7",
  eight: "8",
  nine: "9",
  ten: "10",
  jack: "J",
  queen: "Q",
  king: "K",
  ace: "A",
};

export const isRed = (suit: Suit) => suit === "hearts" || suit === "diamonds";

/** Seats are zero-based on the wire; humans count from one. */
export const seatLabel = (seat: number) => `Seat ${seat + 1}`;

export const cardLabel = (card: Card) => `${RANK_LABEL[card.rank]}${SUIT_GLYPH[card.suit]}`;
