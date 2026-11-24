use std::cmp;
use crate::deck::Deck;
use crate::deck::Card;
use crate::game_event::{*};

pub struct SeatId(u8);
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
        
        for seat in &mut self.seats {
            match seat {
                None => {},
                Some(s) => {
                    s.cards[0] = deck.deal();
                    s.cards[1] = deck.deal();
                }
            }
        }
        self.deck = Some(deck);
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
        Table {
            state: TableState::Idle,
            users: Vec::new(),
            seats: (0..size).map(|_| None).collect(),
            deck: None,
            dealer_pos: None,
            current_pos: None,
            cards: [None, None, None, None, None],
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
        match &mut self.seats[seat as usize] {
            None => None,
            Some(seat) => {
                let bet_amount = cmp::max(bet_amount, seat.bank);
                seat.bank -= bet_amount;
                seat.bet += bet_amount;
                let return_value = seat.bank;
                Some(return_value)
            }
        }
    }
}

#[cfg(test)]
mod tests {
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

}
