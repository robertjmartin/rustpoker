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

    // Chips can sit in the bank or be committed to the bet and the pot, so a
    // seat only runs out of chips once all three are empty.
    fn has_chips(&self) -> bool {
        self.bank + self.bet + self.pot > 0
    }
}

/// Which seats a walk around the table, or a count of them, accepts.
///
/// The variants nest: every `Active` seat is `Occupied`, every `InHand` seat is
/// `Active`, and every `InHandActive` seat is `InHand`.
#[derive(Debug, Clone, Copy)]
enum SeatFilter {
    /// A seat with a player in it, even one who left and is only kept around by
    /// chips still committed to the table.
    Occupied,
    /// A player still in the game: never left and still holding chips.
    Active,
    /// A player in the current hand: active and dealt hole cards.
    InHand,
    /// A player who can still put chips in: in the hand and not all-in.
    InHandActive,
}

impl SeatFilter {
    fn matches(self, seat: &Option<Seat>) -> bool {
        let seat = match seat {
            None => return false,
            Some(seat) => seat,
        };

        match self {
            // Every `Some` seat is occupied; the early return above drops the
            // empty ones.
            SeatFilter::Occupied => true,
            // A leaver is out of the game even while chips they committed to
            // the pot keep their seat from being removed.
            SeatFilter::Active => !seat.left && seat.has_chips(),
            // A left seat can still hold hole cards from the hand in progress;
            // it is not playing them, so it is not in the hand.
            SeatFilter::InHand => !seat.left && seat.cards[0].is_some(),
            // An all-in player has their chips in the bet instead of the bank,
            // so an empty bank is what takes them out of the betting order.
            SeatFilter::InHandActive => !seat.left && seat.cards[0].is_some() && seat.bank > 0,
        }
    }
}

enum TableState {
    Idle,
    PreFlop,
    Bet2,
    Bet3,
    Bet4,
    Bet5,    
}

pub struct Table {
    state: TableState,
    small_blind: u32,
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

    /// The first seat `filter` accepts when walking forward around the table
    /// from `after`, wrapping around once.
    ///
    /// `after` itself is the last seat searched, so passing the current
    /// position finds the next player without skipping a lone occupant. With no
    /// `after`, the walk starts at seat 0 and every seat is a candidate.
    fn next_seat(&self, after: Option<u8>, filter: SeatFilter) -> Option<u8> {
        let seat_count = self.seats.len();
        if seat_count == 0 {
            return None;
        }

        let start = match after {
            None => 0,
            Some(pos) => (pos as usize + 1) % seat_count,
        };

        (0..seat_count)
            .map(|step| (start + step) % seat_count)
            .find(|&idx| filter.matches(&self.seats[idx]))
            .map(|idx| idx as u8)
    }

    /// How many seats `filter` accepts.
    fn count_seats(&self, filter: SeatFilter) -> u8 {
        self.seats
            .iter()
            .filter(|seat| filter.matches(seat))
            .count() as u8
    }

    fn advance_dealer(&mut self) {
        // The button is public state, so the new seat is queued as a table
        // event before the deal: clients learn whose button it is for the same
        // hand they are about to be dealt. A table with no seat in play leaves
        // the button where it is and queues nothing.
        let next = self.next_seat(self.dealer_pos, SeatFilter::Active);

        if let Some(seat) = next {
            self.dealer_pos = Some(seat);
            self.events.push(GameEvent::TableEvent(TableEvent::Dealer(DealerEvent {
                seat: SeatId(seat),
            })));
        }
    }

    /// Post the blinds and point `current_pos` at the first player to act.
    ///
    /// Walking on from the button: the next player at the table posts the small
    /// blind, the one after that the big blind, and the one after *them* opens
    /// the betting. Every step uses `SeatFilter::Active`, so seats emptied by a
    /// leaver are passed over.
    fn post_blinds(&mut self) {
        let blind = self.small_blind;
        let mut after = self.dealer_pos;

        if let Some(seat) = self.next_seat(after, SeatFilter::Active) {
            self.bet(seat, blind);
            after = Some(seat);
        }

        if let Some(seat) = self.next_seat(after, SeatFilter::Active) {
            self.bet(seat, blind * 2);
            after = Some(seat);
        }

        self.current_pos = self.next_seat(after, SeatFilter::Active);
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

                // A hand needs two players who can actually play it. A seat
                // that left while holding chips still occupies the table, but
                // the button, the blinds and the deal all skip it, so it can
                // never be an opponent.
                if self.count_seats(SeatFilter::Active) > 1 {
                    self.advance_dealer();
                    self.post_blinds();
                    self.deal_cards();
                    self.state = TableState::PreFlop;
                }
            },
            TableState::PreFlop => {},
            TableState::Bet2 => {},
            TableState::Bet3 => {},
            TableState::Bet4 => {},
            TableState::Bet5 => {},
        }
    }

    pub fn new(size: u8, small_blind: u32) -> Table {
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
            small_blind,
            users: Vec::new(),
            seats: (0..size).map(|_| None).collect(),
            deck: None,
            dealer_pos: None,
            current_pos: None,
            cards: [None, None, None, None, None],
            events: Vec::new(),
        }
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

        let bet_amount = cmp::min(bet_amount, seat_state.bank);
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
    use crate::table::{SeatFilter, Table};

    #[test]
    fn sit() {
        let mut t = Table::new(8, 10);
        let s1 = t.sit(500);
        let s2 = t.sit(500);
        assert!(s1.is_some());
        assert!(s2.is_some());
        assert_eq!(t.count_seats(SeatFilter::Occupied), 2);
    }

    #[test]
    fn table_full() {
        let mut t = Table::new(2, 10);
        let s1 = t.sit(500);
        let s2 = t.sit(500);
        let s3 = t.sit(500);
        assert!(s1.is_some());
        assert!(s2.is_some());
        assert!(s3.is_none());
        assert_eq!(t.count_seats(SeatFilter::Occupied), 2);
    }

    #[test]
    fn leave() {
        let mut t = Table::new(8, 10);
        assert_eq!(t.sit(500), Some(0));

        // leave only flags the seat: a leaving player may still have chips in the pot.
        assert_eq!(t.leave(0), Some(500));
        assert_eq!(t.count_seats(SeatFilter::Occupied), 1);

        // The seat is freed by the Idle cleanup on the next process() tick.
        t.process();
        assert_eq!(t.count_seats(SeatFilter::Occupied), 0);

        // Leaving an empty seat reports no bank.
        assert_eq!(t.leave(0), None);
    }

    #[test]
    #[should_panic(expected = "exceeds the 16-seat deal mask")]
    fn table_size_is_capped_by_the_deal_mask() {
        Table::new(17, 10);
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

    /// The seat the last `dealer` event in a batch moved the button to.
    fn dealer_seat(events: &[GameEvent]) -> Option<u8> {
        events
            .iter()
            .filter_map(|e| match e {
                GameEvent::TableEvent(TableEvent::Dealer(d)) => Some(d.seat.value()),
                _ => None,
            })
            .last()
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
        let mut t = Table::new(8, 10);
        t.sit(500);
        t.sit(500);
        t.drain_events();

        t.process(); // Idle with two players deals hole cards and moves to PreFlop

        // A deal emits one private hole-card event per dealt seat plus the
        // public deal event.
        let events = t.drain_events();
        assert_eq!(deal_mask(&events), 0b11);
        assert_eq!(hole_card_seats(&events), [0, 1]);
    }

    #[test]
    fn deal_mask_skips_seats_emptied_before_the_deal() {
        let mut t = Table::new(8, 10);
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
        let mut t = Table::new(8, 10);
        t.sit(500); // seat 0, leaves with chips committed
        t.sit(500); // seat 1
        t.sit(500); // seat 2
        t.bet(0, 10); // chips are committed, so the seat is not freed
        t.leave(0);
        t.drain_events();

        t.process();

        let events = t.drain_events();
        assert_eq!(deal_mask(&events), 0b110, "only seats 1 and 2 are dealt");
        assert_eq!(hole_card_seats(&events), [1, 2], "no cards for the left seat");
        assert_eq!(
            t.count_seats(SeatFilter::Occupied),
            3,
            "a seat holding chips still occupies the table"
        );
    }

    /// A seat whose player left but still has chips in front of them cannot be
    /// removed yet, so it stays occupied. It has no opponent to play against,
    /// though, and the deal and the button both already skip it, so it must not
    /// count towards "more than one player" either.
    #[test]
    fn left_seat_with_chips_is_not_an_opponent() {
        let mut t = Table::new(8, 10);
        t.sit(500); // seat 0, the only player still at the table
        t.sit(500); // seat 1
        t.bet(1, 10); // committed chips keep seat 1 seated after leaving
        t.leave(1);
        t.drain_events();

        assert_eq!(
            t.count_seats(SeatFilter::Occupied),
            2,
            "both seats are still occupied"
        );
        assert_eq!(
            t.count_seats(SeatFilter::Active),
            1,
            "only seat 0 is still at the table"
        );

        t.process();

        assert!(
            t.drain_events().is_empty(),
            "one live player is not a hand, even with a left seat still holding chips"
        );
    }

    #[test]
    fn lone_player_does_not_start_a_hand() {
        let mut t = Table::new(8, 10);
        t.sit(500);
        t.drain_events();

        t.process();

        assert!(t.drain_events().is_empty());
    }

    #[test]
    fn deal_happens_once_per_hand() {
        let mut t = Table::new(8, 10);
        t.sit(500);
        t.sit(500);
        t.drain_events();

        // Idle -> PreFlop: the button, the two blinds, two hole cards and the deal.
        t.process();
        assert_eq!(t.drain_events().len(), 6);

        // PreFlop has no betting logic yet, and in particular does not deal again.
        t.process();
        assert!(t.drain_events().is_empty());
    }

    /// The chips in `seat`'s bank and in front of them.
    fn chips(t: &Table, seat: usize) -> (u32, u32) {
        match &t.seats[seat] {
            None => panic!("seat {seat} is empty"),
            Some(s) => (s.bank, s.bet),
        }
    }

    #[test]
    fn blinds_are_posted_and_the_first_to_act_follows_the_big_blind() {
        let mut t = Table::new(8, 10);
        t.sit(500); // seat 0, the button
        t.sit(500); // seat 1
        t.sit(500); // seat 2
        t.drain_events();

        t.process(); // Idle -> PreFlop

        assert_eq!(t.dealer_pos, Some(0));
        assert_eq!(chips(&t, 1), (490, 10), "seat 1 posts the small blind");
        assert_eq!(chips(&t, 2), (480, 20), "seat 2 posts the big blind");
        assert_eq!(chips(&t, 0), (500, 0), "the button posts nothing");
        assert_eq!(t.current_pos, Some(0), "seat 0 is first to act");
    }

    #[test]
    fn a_short_stack_posts_a_short_blind() {
        let mut t = Table::new(8, 10);
        t.sit(500); // seat 0, the button
        t.sit(5); // seat 1, too short to cover the small blind
        t.drain_events();

        t.process();

        assert_eq!(chips(&t, 1), (0, 5), "only the chips they have go in");
        assert_eq!(chips(&t, 0), (480, 20), "the button still posts the big blind");
        assert_eq!(t.current_pos, Some(1), "seat 1 acts first after the big blind");
    }

    #[test]
    fn sit_reuses_the_lowest_freed_seat() {
        let mut t = Table::new(8, 10);
        assert_eq!(t.sit(500), Some(0));
        assert_eq!(t.sit(500), Some(1));

        assert_eq!(t.leave(0), Some(500));
        t.process(); // frees seat 0; one player left, so no deal

        assert_eq!(t.sit(500), Some(0));
    }

    #[test]
    fn dealer_starts_at_the_first_seated_player() {
        let mut t = Table::new(8, 10);
        t.sit(500);
        t.sit(500);

        t.drain_events();
        t.advance_dealer();
        assert_eq!(t.dealer_pos, Some(0));

        let events = t.drain_events();
        assert_eq!(dealer_seat(&events), Some(0), "the button is announced to clients");
    }

    #[test]
    fn dealer_skips_seats_that_have_left_and_wraps_around() {
        let mut t = Table::new(8, 10);
        t.sit(500); // seat 0
        t.sit(500); // seat 1
        t.sit(500); // seat 2
        t.bet(1, 10); // chips committed, so cleanup keeps seat 1 around
        t.leave(1);
        t.drain_events();

        t.advance_dealer();
        assert_eq!(t.dealer_pos, Some(0));

        t.advance_dealer();
        assert_eq!(t.dealer_pos, Some(2), "seat 1 left the table");
        let events = t.drain_events();
        assert_eq!(dealer_seat(&events), Some(2));

        t.advance_dealer();
        assert_eq!(t.dealer_pos, Some(0), "wraps around to the first seat");
    }

    #[test]
    fn next_seat_can_look_past_seats_that_are_not_in_the_hand() {
        let mut t = Table::new(8, 10);
        t.sit(500); // seat 0
        t.sit(500); // seat 1
        t.sit(500); // seat 2
        t.deal_cards(); // every seated player gets hole cards
        t.seats[1].as_mut().unwrap().cards = [None, None]; // seat 1 sits the hand out
        t.drain_events();

        assert_eq!(
            t.next_seat(Some(0), SeatFilter::Active),
            Some(1),
            "seat 1 is at the table even without cards"
        );
        assert_eq!(
            t.next_seat(Some(0), SeatFilter::InHand),
            Some(2),
            "seat 1 skipped: it has no cards"
        );
        assert_eq!(t.next_seat(Some(2), SeatFilter::InHand), Some(0), "wraps around");
        assert_eq!(t.next_seat(None, SeatFilter::InHand), Some(0));
    }

    #[test]
    fn next_seat_can_skip_players_who_are_all_in() {
        let mut t = Table::new(8, 10);
        t.sit(500); // seat 0
        t.sit(500); // seat 1
        t.sit(500); // seat 2
        t.deal_cards();
        t.bet(1, 500); // seat 1 commits every chip: bank empty, all-in
        t.drain_events();

        assert_eq!(
            t.next_seat(Some(0), SeatFilter::InHand),
            Some(1),
            "an all-in player is still in the hand"
        );
        assert_eq!(
            t.next_seat(Some(0), SeatFilter::InHandActive),
            Some(2),
            "seat 1 is skipped: all-in players cannot bet"
        );
        assert_eq!(t.next_seat(Some(2), SeatFilter::InHandActive), Some(0), "wraps around");
        assert_eq!(t.next_seat(None, SeatFilter::InHandActive), Some(0));
    }

    #[test]
    fn next_seat_ignores_a_seat_that_left_but_still_has_chips_and_cards() {
        let mut t = Table::new(8, 10);
        t.sit(500); // seat 0
        t.sit(500); // seat 1
        t.deal_cards();
        t.bet(1, 10); // committed chips keep the seat around
        t.leave(1);
        t.drain_events();

        assert_eq!(t.next_seat(Some(0), SeatFilter::Active), Some(0));
        assert_eq!(t.next_seat(Some(0), SeatFilter::InHand), Some(0));
    }

    #[test]
    fn dealer_keeps_the_button_when_every_other_seat_is_out() {
        let mut t = Table::new(8, 10);
        t.sit(500);

        t.drain_events();
        t.advance_dealer();
        assert_eq!(t.dealer_pos, Some(0));
        t.drain_events();

        t.advance_dealer();
        assert_eq!(t.dealer_pos, Some(0));
        let events = t.drain_events();
        assert_eq!(dealer_seat(&events), Some(0), "a lone player keeps the button");
    }

    #[test]
    fn no_dealer_event_without_a_hand() {
        let mut t = Table::new(8, 10);
        t.sit(500);
        t.drain_events();

        t.process(); // a lone player does not start a hand
        assert!(t.drain_events().is_empty());
    }

    #[test]
    fn bet_on_an_empty_seat_returns_none() {
        let mut t = Table::new(8, 10);

        assert_eq!(t.bet(0, 10), None);
        assert!(t.drain_events().is_empty());
    }

    #[test]
    fn events_are_queued_and_drained() {
        let mut t = Table::new(8, 10);
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
