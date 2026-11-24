use crate::deck::{Card};
use crate::table::{SeatId, BetAmount};

pub enum GameEvent {
    Sit(SitEvent),
    Deal(DealEvent),
    Bet(BetEvent),
    Fold(FoldEvent),
    DummyEvent,
}

pub struct SitEvent {
    pub seat: SeatId,
}

pub struct DealEvent {
    pub seats_dealt: Vec<SeatId>,
}

pub struct BetEvent {
    pub seat: SeatId,
    pub amount: BetAmount,
}

pub struct FoldEvent {
    pub seat: SeatId,
}

pub struct HoleCardEvent {
    pub cards: [Card; 2],
}
