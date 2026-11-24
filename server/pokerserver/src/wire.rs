use serde::Serialize;
use pokercore::GameEvent;

/// Wire representation of a `pokercore::GameEvent`.
#[derive(Debug, Serialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum GameEventWire {
    Sit { seat: u8 },
    Deal { seats_dealt: Vec<u8> },
    Bet { seat: u8, amount: u32 },
    Fold { seat: u8 },
    DummyEvent,
}

impl From<&GameEvent> for GameEventWire {
    fn from(event: &GameEvent) -> Self {
        match event {
            GameEvent::Sit(e) => GameEventWire::Sit { seat: e.seat.value() },
            GameEvent::Deal(e) => GameEventWire::Deal {
                seats_dealt: e.seats_dealt.iter().map(|seat| seat.value()).collect(),
            },
            GameEvent::Bet(e) => GameEventWire::Bet {
                seat: e.seat.value(),
                amount: e.amount.value(),
            },
            GameEvent::Fold(e) => GameEventWire::Fold { seat: e.seat.value() },
            GameEvent::DummyEvent => GameEventWire::DummyEvent,
        }
    }
}
