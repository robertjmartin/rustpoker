use crate::deck::{Card};
use crate::table::{SeatId, BetAmount};

#[derive(Debug, Clone)]
pub enum GameEvent {
    Sit(SitEvent),
    Deal(DealEvent),
    Bet(BetEvent),
    Fold(FoldEvent),
    DummyEvent,
}

#[derive(Debug, Clone)]
pub struct SitEvent {
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

pub struct HoleCardEvent {
    pub cards: [Card; 2],
}
