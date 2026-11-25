use std::cmp;
use crate::deck::Deck;
use crate::deck::Card;
use crate::game_event::{*};

#[derive(Debug, Clone, Copy)]
pub struct SeatId(u8);
#[derive(Debug, Clone, Copy)]
pub struct BetAmount(u32);

impl SeatId {
    pub fn value(&self) -> u8 {
        self.0
    }
}

impl BetAmount {
    pub fn value(&self) -> u32 {
        self.0
    }
}

struct User {
    seat: Option<SeatId>,
    game_event_callback: fn(GameEvent),
    hole_card_callback: fn(HoleCardEvent),
}

struct Seat {
    bank: u32,
    bet: u32,
    pot: u32,
    left: bool,
    cards: [Option<Card>; 2],
}

impl Seat {
    fn new(bank: u32) -> Seat {
        Seat {
            bank, 
            bet: 0,
            pot: 0,
            left: false,
            cards: [None, None]
        }
    }
}

enum TableState {
    Idle,
    Bet1,
    Bet2,
    Bet3,
    Bet4,
    Bet5,    
}

pub struct Table {
    state: TableState,
    users: Vec<Option<User>>,
    seats: Vec<Option<Seat>>,
    deck: Option<Deck>,
    dealer_pos: Option<u8>,
    current_pos: Option<u8>,
    cards: [Option<Card>; 5],
    events: Vec<GameEvent>,
}

impl Table {
    fn remove_players(&mut self) {
        for seat in &mut self.seats {
            match seat {
                None => {},
                Some(s) => {
                    if s.left == true && s.bet == 0 && s.pot == 0 {
                        *seat = None
                    }
                }
            }
        }
    }

    fn deal_cards(&mut self) {
        let mut deck = Deck::new();
        deck.shuffle();
        
        let mut seats_dealt = 0u16;
        for (idx, seat) in self.seats.iter_mut().enumerate() {
            match seat {
                None => {},
                Some(s) => {
                    if !s.left {
                        let card1 = deck.deal().unwrap();
                        let card2 = deck.deal().unwrap();

                        seats_dealt |= 1 << idx;
                        self.events.push(GameEvent::HoleCardEvent(HoleCardEvent {seat: SeatId(idx as u8), cards: [card1.clone(), card2.clone()]}));
                        s.cards[0] = Some(card1);
                        s.cards[1] = Some(card2);
                    }
                }
            }
        }
        self.deck = Some(deck);
        self.events.push(GameEvent::TableEvent(TableEvent::Deal(DealEvent { seats_dealt })));
    }

    pub fn process(&mut self) {
        println!("Table process");
        match self.state {
            TableState::Idle => {
                self.remove_players();

                if self.player_count() > 1 {
                    self.deal_cards();
                    //self.seats.iter().position(|s| s.)
                    self.state = TableState::Bet1;
                }
            },
            TableState::Bet1 => {},
            TableState::Bet2 => {},
            TableState::Bet3 => {},
            TableState::Bet4 => {},
            TableState::Bet5 => {},
        }
    }

    pub fn new(size: u8) -> Table {
        // `DealEvent::seats_dealt` is a bitmask with one bit per seat, so the
        // table can never be wider than the mask. Tied to the mask's type so
        // the two cannot drift apart.
        assert!(
            size <= u16::BITS as u8,
            "table size {size} exceeds the {}-seat deal mask",
            u16::BITS
        );

        Table {
            state: TableState::Idle,
            users: Vec::new(),
            seats: (0..size).map(|_| None).collect(),
            deck: None,
            dealer_pos: None,
            current_pos: None,
            cards: [None, None, None, None, None],
            events: Vec::new(),
        }
    }

    pub fn player_count(&self) -> u8 {
        let mut count: u8 = 0;
        for seat in &self.seats {
            match seat {
                Some(_) => count+=1,
                None => {}
            }
        }
        count
    }

    pub fn sit(&mut self, bank: u32) -> Option<u8> {
        let empty_seat = self.seats.iter().position(|s| s.is_none());

        match empty_seat {
            Some(idx) => {
                self.seats[idx] = Some(Seat::new(bank));
                self.events.push(GameEvent::TableEvent(TableEvent::Sit(SitEvent { seat: SeatId(idx as u8) })));
                Some(idx as u8)
            }
            None => None, // table is full
        }
    }

    // return bank balance, None if no player
    pub fn leave(&mut self, seat: u8) -> Option<u32> {
        match &mut self.seats[seat as usize] {
            None => None,
            Some(seat) => {
                seat.left = true;
                let return_value = seat.bank;
                Some(return_value)
            }
        }
    }

    pub fn bet(&mut self, seat: u8, bet_amount: u32) -> Option<u32> {
        let seat_state = match &mut self.seats[seat as usize] {
            None => return None,
            Some(seat_state) => seat_state,
        };

        let bet_amount = cmp::max(bet_amount, seat_state.bank);
        seat_state.bank -= bet_amount;
        seat_state.bet += bet_amount;

        self.events.push(GameEvent::TableEvent(TableEvent::Bet(BetEvent {
            seat: SeatId(seat),
            amount: BetAmount(bet_amount),
        })));

        Some(seat_state.bank)
    }

    pub fn drain_events(&mut self) -> Vec<GameEvent> {
        std::mem::take(&mut self.events)
    }
}

#[cfg(test)]
mod tests {
    use crate::game_event::{GameEvent, TableEvent};
    use crate::table::Table;

    #[test]
    fn sit() {
        let mut t = Table::new(8);
        let s1 = t.sit(500);
        let s2 = t.sit(500);
        assert!(s1.is_some());
        assert!(s2.is_some());
        assert_eq!(t.player_count(), 2);
    }

    #[test]
    fn table_full() {
        let mut t = Table::new(2);
        let s1 = t.sit(500);
        let s2 = t.sit(500);
        let s3 = t.sit(500);
        assert!(s1.is_some());
        assert!(s2.is_some());
        assert!(s3.is_none());
        assert_eq!(t.player_count(), 2);
    }

    #[test]
    fn leave() {
        let mut t = Table::new(8);
        assert_eq!(t.sit(500), Some(0));

        // leave only flags the seat: a leaving player may still have chips in the pot.
        assert_eq!(t.leave(0), Some(500));
        assert_eq!(t.player_count(), 1);

        // The seat is freed by the Idle cleanup on the next process() tick.
        t.process();
        assert_eq!(t.player_count(), 0);

        // Leaving an empty seat reports no bank.
        assert_eq!(t.leave(0), None);
    }

    #[test]
    #[should_panic(expected = "exceeds the 16-seat deal mask")]
    fn table_size_is_capped_by_the_deal_mask() {
        Table::new(17);
    }

    /// The seat bitmask of the public `Deal` event in a batch of events.
    fn deal_mask(events: &[GameEvent]) -> u16 {
        events
            .iter()
            .find_map(|e| match e {
                GameEvent::TableEvent(TableEvent::Deal(d)) => Some(d.seats_dealt),
                _ => None,
            })
            .expect("deal event")
    }

    /// The seats that got a private hole-card event, in queue order.
    fn hole_card_seats(events: &[GameEvent]) -> Vec<u8> {
        events
            .iter()
            .filter_map(|e| match e {
                GameEvent::HoleCardEvent(h) => Some(h.seat.value()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn deal_sets_one_bit_per_seated_player() {
        let mut t = Table::new(8);
        t.sit(500);
        t.sit(500);
        t.drain_events();

        t.process(); // Idle with two players deals hole cards and moves to Bet1

        // A deal emits one private hole-card event per dealt seat plus the
        // public deal event.
        let events = t.drain_events();
        assert_eq!(deal_mask(&events), 0b11);
        assert_eq!(hole_card_seats(&events), [0, 1]);
    }

    #[test]
    fn deal_mask_skips_seats_emptied_before_the_deal() {
        let mut t = Table::new(8);
        t.sit(500);
        t.sit(500);
        t.sit(500);
        t.leave(1); // no chips committed, so the Idle cleanup frees seat 1
        t.drain_events();

        t.process();

        let events = t.drain_events();
        assert_eq!(deal_mask(&events), 0b101);
        assert_eq!(hole_card_seats(&events), [0, 2]);
    }

    #[test]
    fn left_seat_with_chips_is_skipped_by_the_deal() {
        let mut t = Table::new(8);
        t.sit(500);
        t.sit(500);
        t.bet(0, 10); // chips are committed, so the seat is not freed
        t.leave(0);
        t.drain_events();

        t.process();

        let events = t.drain_events();
        assert_eq!(deal_mask(&events), 0b10, "only seat 1 is dealt");
        assert_eq!(hole_card_seats(&events), [1], "no cards for the left seat");
        assert_eq!(
            t.player_count(),
            2,
            "a seat holding chips still occupies the table"
        );
    }

    #[test]
    fn lone_player_does_not_start_a_hand() {
        let mut t = Table::new(8);
        t.sit(500);
        t.drain_events();

        t.process();

        assert!(t.drain_events().is_empty());
    }

    #[test]
    fn deal_happens_once_per_hand() {
        let mut t = Table::new(8);
        t.sit(500);
        t.sit(500);
        t.drain_events();

        t.process(); // Idle -> Bet1: two hole cards and the deal
        assert_eq!(t.drain_events().len(), 3);

        // Bet1 has no betting logic yet, and in particular does not deal again.
        t.process();
        assert!(t.drain_events().is_empty());
    }

    #[test]
    fn sit_reuses_the_lowest_freed_seat() {
        let mut t = Table::new(8);
        assert_eq!(t.sit(500), Some(0));
        assert_eq!(t.sit(500), Some(1));

        assert_eq!(t.leave(0), Some(500));
        t.process(); // frees seat 0; one player left, so no deal

        assert_eq!(t.sit(500), Some(0));
    }

    #[test]
    fn bet_on_an_empty_seat_returns_none() {
        let mut t = Table::new(8);

        assert_eq!(t.bet(0, 10), None);
        assert!(t.drain_events().is_empty());
    }

    #[test]
    fn events_are_queued_and_drained() {
        let mut t = Table::new(8);
        t.sit(500);
        t.sit(500);
        t.bet(0, 10);

        let events = t.drain_events();
        assert_eq!(events.len(), 3);
        assert!(matches!(&events[0], GameEvent::TableEvent(TableEvent::Sit(_))));
        assert!(matches!(&events[1], GameEvent::TableEvent(TableEvent::Sit(_))));
        assert!(matches!(&events[2], GameEvent::TableEvent(TableEvent::Bet(_))));

        // Draining empties the queue.
        assert!(t.drain_events().is_empty());
    }
}
