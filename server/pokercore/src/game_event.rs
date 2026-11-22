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
    seat: SeatId,
}

pub struct DealEvent {
    seats_dealt: Vec<SeatId>,
}

pub struct BetEvent {
    seat: SeatId,
    amount: BetAmount,
}

pub struct FoldEvent {
    seat: SeatId,
}

pub struct HoleCardEvent {
    cards: [Card; 2],
}
