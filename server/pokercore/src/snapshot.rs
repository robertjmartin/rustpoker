use serde::Serialize;

use crate::deck::Card;

/// A point-in-time view of one table, built for one viewer.
///
/// Everything is public except `your_cards`, which is only filled for the
/// viewer's own seat. That makes the same frame safe to send privately to a
/// joining connection without revealing anybody else's hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableSnapshot {
    /// One entry per seat, including empty ones.
    pub seats: Vec<SeatSnapshot>,
    /// Community cards dealt so far.
    pub board: Vec<Card>,
    pub pot: u32,
    /// Highest contribution in the current betting round.
    pub current_bet: u32,
    /// The table's forced bets, so a joining client can show the stakes.
    pub small_blind: u32,
    pub big_blind: u32,
    pub ante: u32,
    /// Seat to act, if a hand is running.
    pub acting: Option<u8>,
    /// Dealer button seat, if a hand has started.
    pub dealer: Option<u8>,
    pub street: StreetName,
    pub your_seat: Option<u8>,
    pub your_cards: Option<[Card; 2]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeatSnapshot {
    pub seat: u8,
    /// False for an empty seat and for a player who stood up mid-hand.
    pub occupied: bool,
    /// Chips behind, excluding the current street's bet.
    pub stack: u32,
    /// Chips put in during the current betting round.
    pub bet: u32,
    pub folded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StreetName {
    Idle,
    PreFlop,
    Flop,
    Turn,
    River,
}
