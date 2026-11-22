use serde::Serialize;

use crate::deck::Card;
use crate::table::{SeatId, BetAmount};

/// Events that are visible to everyone joined to the table.
///
/// Serialises as a tagged object, e.g. `{"event":"bet","seat":3,"amount":50}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum GameEvent {
    Sit(SitEvent),
    Leave(LeaveEvent),
    Deal(DealEvent),
    Ante(AnteEvent),
    Blind(BlindEvent),
    Bet(BetEvent),
    Check(CheckEvent),
    Fold(FoldEvent),
    Turn(TurnEvent),
    CommunityCards(CommunityCardsEvent),
    Showdown(ShowdownEvent),
    HandEnd(HandEndEvent),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SitEvent {
    pub seat: SeatId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaveEvent {
    pub seat: SeatId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DealEvent {
    pub seats_dealt: Vec<SeatId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BetEvent {
    pub seat: SeatId,
    pub amount: BetAmount,
}

/// Forced ante paid by every player dealt into the hand. Dead money in the pot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnteEvent {
    pub seat: SeatId,
    pub amount: BetAmount,
}

/// A forced blind, posted before preflop action. A blind is live: it counts as
/// that player's current-round contribution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlindEvent {
    pub seat: SeatId,
    pub amount: BetAmount,
    pub kind: BlindKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BlindKind {
    Small,
    Big,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoldEvent {
    pub seat: SeatId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckEvent {
    pub seat: SeatId,
}

/// Whose turn it is. Sent whenever the action moves to another seat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnEvent {
    pub seat: SeatId,
}

/// Cards added to the board when a street opens (3 for the flop, then 1 each).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunityCardsEvent {
    pub cards: Vec<Card>,
}

/// The hands still in the pot at showdown, published just before `HandEnd`.
/// Players who folded are not included, so mucked hands stay private.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowdownEvent {
    pub hands: Vec<ShowdownHand>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowdownHand {
    pub seat: SeatId,
    pub cards: [Card; 2],
}

/// The pot has been awarded and the hand is over.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandEndEvent {
    pub winners: Vec<SeatId>,
    pub amount: u32,
    pub showdown: bool,
}

/// Private to one player. Deliberately not part of `GameEvent` so it can be
/// delivered to its owner only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoleCardEvent {
    pub cards: [Card; 2],
}
