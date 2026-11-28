use crate::deck::Card;
use crate::table::{SeatId, BetAmount};

/// Top-level event emitted by the table.
///
/// `TableEvent`s are public: the server broadcasts them to every connection.
/// `HoleCardEvent`s are secret and belong to a single player, so whatever
/// carries this value has to pick a recipient.
#[derive(Debug, Clone)]
pub enum GameEvent {
    TableEvent(TableEvent),
    HoleCardEvent(HoleCardEvent),
}

#[derive(Debug, Clone)]
pub enum TableEvent {
    Sit(SitEvent),
    Dealer(DealerEvent),
    Deal(DealEvent),
    Bet(BetEvent),
    Fold(FoldEvent),
}

#[derive(Debug, Clone)]
pub struct SitEvent {
    pub seat: SeatId,
}

#[derive(Debug, Clone)]
pub struct DealerEvent {
    /// The seat the button moved to for the hand about to start.
    pub seat: SeatId,
}

#[derive(Debug, Clone)]
pub struct DealEvent {
    /// Bitmask of the seats that received hole cards: bit N means seat N.
    ///
    /// A mask instead of a `Vec<SeatId>`
    pub seats_dealt: u16,
}

#[derive(Debug, Clone)]
pub struct BetEvent {
    pub seat: SeatId,
    pub amount: BetAmount,
}

#[derive(Debug, Clone)]
pub struct FoldEvent {
    pub seat: SeatId,
}

#[derive(Debug, Clone)]
pub struct HoleCardEvent {
    /// The seat these cards belong to. Only that player may see the event.
    pub seat: SeatId,
    pub cards: [Card; 2],
}
