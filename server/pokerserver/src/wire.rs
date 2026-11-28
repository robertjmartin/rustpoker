use serde::ser::{SerializeSeq, Serializer};
use serde::Serialize;
use pokercore::{GameEvent, HoleCardEvent, TableEvent};

/// Wire representation of a `pokercore::GameEvent`.
#[derive(Debug, Serialize)]
#[serde(tag = "event", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum GameEventWire {
    Sit { seat: u8 },
    Dealer { seat: u8 },
    Deal { seats_dealt: SeatBits },
    Bet { seat: u8, amount: u32 },
    Fold { seat: u8 },
    HoleCards { seat: u8, cards: [CardWire; 2] },
}

impl From<&TableEvent> for GameEventWire {
    fn from(event: &TableEvent) -> Self {
        match event {
            TableEvent::Sit(e) => GameEventWire::Sit { seat: e.seat.value() },
            TableEvent::Dealer(e) => GameEventWire::Dealer { seat: e.seat.value() },
            TableEvent::Deal(e) => GameEventWire::Deal {
                seats_dealt: SeatBits(e.seats_dealt),
            },
            TableEvent::Bet(e) => GameEventWire::Bet {
                seat: e.seat.value(),
                amount: e.amount.value(),
            },
            TableEvent::Fold(e) => GameEventWire::Fold { seat: e.seat.value() },
        }
    }
}

impl From<&GameEvent> for GameEventWire {
    fn from(event: &GameEvent) -> Self {
        match event {
            GameEvent::TableEvent(e) => e.into(),
            GameEvent::HoleCardEvent(e) => e.into(),
        }
    }
}

impl From<&HoleCardEvent> for GameEventWire {
    fn from(event: &HoleCardEvent) -> Self {
        let cards = std::array::from_fn(|i| {
            let (rank, suit) = event.cards[i].get_value();
            CardWire {
                rank: format!("{rank:?}"),
                suit: format!("{suit:?}"),
            }
        });
        GameEventWire::HoleCards {
            seat: event.seat.value(),
            cards,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CardWire {
    rank: String,
    suit: String,
}

/// Serialises the dealt-seats bitmask as the JSON array the protocol already
/// uses (`[0, 1]`). Writing the sequence element by element avoids the
/// intermediate `Vec<u8>` that a `collect()` would allocate for every frame.
#[derive(Debug)]
pub struct SeatBits(u16);

impl Serialize for SeatBits {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(None)?;
        for seat in 0..u16::BITS as u8 {
            if self.0 & (1u16 << seat) != 0 {
                seq.serialize_element(&seat)?;
            }
        }
        seq.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dealer_serialises_the_button_seat() {
        let wire = GameEventWire::Dealer { seat: 3 };
        assert_eq!(
            serde_json::to_string(&wire).unwrap(),
            r#"{"event":"dealer","seat":3}"#
        );
    }

    #[test]
    fn deal_bitmask_serialises_as_a_seat_array() {
        let wire = GameEventWire::Deal { seats_dealt: SeatBits(0b101) };
        assert_eq!(
            serde_json::to_string(&wire).unwrap(),
            r#"{"event":"deal","seatsDealt":[0,2]}"#
        );
    }
}
