use std::cmp;
use serde::Serialize;
use crate::deck::Deck;
use crate::deck::Card;
use crate::game_event::*;
use crate::hand::best_hand;
use crate::hand::HandRank;
use crate::snapshot::{SeatSnapshot, StreetName, TableSnapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SeatId(pub u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BetAmount(pub u32);

struct User {
    seat: Option<SeatId>,
    game_event_callback: fn(GameEvent),
    hole_card_callback: fn(HoleCardEvent),
}

struct Seat {
    bank: u32,
    /// Chips put in during the current betting round. They move into the
    /// table pot at the end of the round.
    bet: u32,
    left: bool,
    folded: bool,
    /// Has acted at least once in the current betting round.
    acted: bool,
    cards: [Option<Card>; 2],
}

impl Seat {
    fn new(bank: u32) -> Seat {
        Seat {
            bank,
            bet: 0,
            left: false,
            folded: false,
            acted: false,
            cards: [None, None],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TableState {
    Idle,
    PreFlop,
    Flop,
    Turn,
    River,
}

pub struct Table {
    state: TableState,
    users: Vec<Option<User>>,
    seats: Vec<Option<Seat>>,
    deck: Option<Deck>,
    dealer_pos: Option<u8>,
    current_pos: Option<u8>,
    cards: [Option<Card>; 5],
    pot: u32,
    /// Highest per-player contribution in the current betting round.
    current_bet: u32,
    /// Forced bets: the small blind, the big blind and the per-player ante.
    small_blind: u32,
    big_blind: u32,
    ante: u32,
    /// One-second ticks left for the seat `current_pos` to act.
    turn_ticks_left: u8,
    events: Vec<GameEvent>,
    hole_cards: Vec<(SeatId, HoleCardEvent)>,
}

impl Table {
    /// One-second ticks a player gets before the table acts for them.
    pub const TURN_TICKS: u8 = 30;

    pub const DEFAULT_SMALL_BLIND: u32 = 10;
    pub const DEFAULT_BIG_BLIND: u32 = 20;
    pub const DEFAULT_ANTE: u32 = 2;

    pub fn new(size: u8) -> Table {
        Table::with_stakes(size, Self::DEFAULT_SMALL_BLIND, Self::DEFAULT_BIG_BLIND, Self::DEFAULT_ANTE)
    }

    /// A table with explicit forced bets. The big blind is raised to the small
    /// blind if the pair is inverted, so preflop never opens below the small
    /// blind.
    pub fn with_stakes(size: u8, small_blind: u32, big_blind: u32, ante: u32) -> Table {
        Table {
            state: TableState::Idle,
            users: Vec::new(),
            seats: (0..size).map(|_| None).collect(),
            deck: None,
            dealer_pos: None,
            current_pos: None,
            cards: [None, None, None, None, None],
            pot: 0,
            current_bet: 0,
            small_blind,
            big_blind: big_blind.max(small_blind),
            ante,
            turn_ticks_left: 0,
            events: Vec::new(),
            hole_cards: Vec::new(),
        }
    }

    // Number of players still at the table: seats flagged `left` are not counted.
    pub fn player_count(&self) -> u8 {
        let mut count: u8 = 0;
        for seat in &self.seats {
            match seat {
                Some(s) if !s.left => count+=1,
                _ => {}
            }
        }
        count
    }

    pub fn sit(&mut self, bank: u32) -> Option<u8> {
        let empty_seat = self.seats.iter().position(|s| s.is_none());

        match empty_seat {
            Some(idx) => {
                let mut seat = Seat::new(bank);
                // A player who sits down mid-hand waits for the next one.
                seat.folded = !matches!(self.state, TableState::Idle);
                self.seats[idx] = Some(seat);
                self.events.push(GameEvent::Sit(SitEvent { seat: SeatId(idx as u8) }));
                Some(idx as u8)
            }
            None => None, // table is full
        }
    }

    // return bank balance, None if no player
    pub fn leave(&mut self, seat: u8) -> Option<u32> {
        match self.seats.get_mut(seat as usize) {
            None | Some(None) => None,
            Some(Some(s)) => {
                if !s.left {
                    s.left = true;
                    s.folded = true;
                    self.events.push(GameEvent::Leave(LeaveEvent { seat: SeatId(seat) }));
                }
                Some(s.bank)
            }
        }
    }

    // return remaining bank, None if the action is not legal right now.
    pub fn bet(&mut self, seat: u8, bet_amount: u32) -> Option<u32> {
        if self.current_pos != Some(seat) || !self.is_betting() {
            return None;
        }

        let idx = seat as usize;
        let (amount, remaining, seat_bet) = match self.seats.get_mut(idx) {
            Some(Some(s)) => {
                if s.folded || s.left || s.bank == 0 {
                    return None;
                }
                let amount = cmp::min(bet_amount, s.bank);
                if amount == 0 {
                    return None;
                }
                s.bank -= amount;
                s.bet += amount;
                s.acted = true;
                (amount, s.bank, s.bet)
            }
            _ => return None,
        };

        self.pot += amount;
        self.current_bet = cmp::max(self.current_bet, seat_bet);
        self.events.push(GameEvent::Bet(BetEvent { seat: SeatId(seat), amount: BetAmount(amount) }));
        self.after_action();
        Some(remaining)
    }

    /// Pass the action. Only legal when there is nothing to call.
    pub fn check(&mut self, seat: u8) -> bool {
        if self.current_pos != Some(seat) || !self.is_betting() {
            return false;
        }

        let current_bet = self.current_bet;
        match self.seats.get_mut(seat as usize) {
            Some(Some(s)) => {
                if s.folded || s.left || s.bet < current_bet {
                    return false;
                }
                s.acted = true;
            }
            _ => return false,
        }

        self.events.push(GameEvent::Check(CheckEvent { seat: SeatId(seat) }));
        self.after_action();
        true
    }

    pub fn fold(&mut self, seat: u8) -> bool {
        if self.current_pos != Some(seat) || !self.is_betting() {
            return false;
        }

        match self.seats.get_mut(seat as usize) {
            Some(Some(s)) => {
                if s.folded || s.left {
                    return false;
                }
                s.folded = true;
                s.acted = true;
            }
            _ => return false,
        }

        self.events.push(GameEvent::Fold(FoldEvent { seat: SeatId(seat) }));
        self.after_action();
        true
    }

    /// Advance the table. Called once a second by the server: runs the turn
    /// clock, starts a hand when enough players are seated and runs out
    /// streets that nobody can act on any more.
    pub fn process(&mut self) {
        if self.is_betting() {
            if let Some(seat) = self.current_pos {
                self.turn_ticks_left = self.turn_ticks_left.saturating_sub(1);
                if self.turn_ticks_left == 0 {
                    self.auto_act(seat);
                    // A timeout can finish the hand. Deal the next one on the
                    // next tick so clients get a chance to see the result.
                    if self.state == TableState::Idle {
                        return;
                    }
                }
            }
        }

        match self.state {
            TableState::Idle => self.start_hand(),
            _ => {
                if self.players_in_hand() <= 1 {
                    self.end_hand(false);
                    return;
                }
                let mut guard = 0;
                while self.state != TableState::Idle && self.round_complete() && guard < 8 {
                    self.end_street();
                    guard += 1;
                }
            }
        }
    }

    /// What the table does when a turn clock expires: check when there is
    /// nothing to call, otherwise fold.
    fn auto_act(&mut self, seat: u8) {
        let can_check = matches!(
            self.seats.get(seat as usize),
            Some(Some(s)) if !s.folded && !s.left && s.bet >= self.current_bet
        );
        if !(can_check && self.check(seat)) {
            self.fold(seat);
        }
    }

    /// Take the events produced since the last call. The server drains this
    /// after every command and every tick and fans the events out to clients.
    pub fn drain_events(&mut self) -> Vec<GameEvent> {
        std::mem::take(&mut self.events)
    }

    /// Take the hole cards dealt since the last call, one entry per seat. They
    /// are kept out of `drain_events` so the server can send each hand only to
    /// the player holding that seat.
    pub fn drain_hole_cards(&mut self) -> Vec<(SeatId, HoleCardEvent)> {
        std::mem::take(&mut self.hole_cards)
    }

    /// The two cards held at `seat`, if that seat is occupied and has been dealt
    /// in. Used when a client asks for its hand after joining or reconnecting.
    pub fn hole_cards(&self, seat: u8) -> Option<HoleCardEvent> {
        match self.seats.get(seat as usize) {
            Some(Some(s)) => match s.cards {
                [Some(first), Some(second)] => Some(HoleCardEvent { cards: [first, second] }),
                _ => None,
            },
            _ => None,
        }
    }

    /// A view of the table for one viewer.
    ///
    /// Seats, stacks, bets, the board, the pot and the actor are public; the
    /// viewer's own two cards are added only when that seat is still at the
    /// table. The server sends this privately on join so a late or reconnecting
    /// client does not have to wait for the next event to be coherent.
    pub fn snapshot(&self, viewer: Option<u8>) -> TableSnapshot {
        let seats = self
            .seats
            .iter()
            .enumerate()
            .map(|(index, slot)| match slot {
                Some(seat) => SeatSnapshot {
                    seat: index as u8,
                    occupied: !seat.left,
                    stack: if seat.left { 0 } else { seat.bank },
                    bet: if seat.left { 0 } else { seat.bet },
                    folded: seat.folded,
                },
                None => SeatSnapshot {
                    seat: index as u8,
                    occupied: false,
                    stack: 0,
                    bet: 0,
                    folded: false,
                },
            })
            .collect();

        // A player who stood up is not shown their old hand again.
        let your_seat = viewer.filter(|&seat| match self.seats.get(seat as usize) {
            Some(Some(s)) => !s.left,
            _ => false,
        });
        let your_cards = your_seat.and_then(|seat| self.hole_cards(seat)).map(|hand| hand.cards);

        TableSnapshot {
            seats,
            board: self.cards.iter().flatten().copied().collect(),
            pot: self.pot,
            current_bet: self.current_bet,
            small_blind: self.small_blind,
            big_blind: self.big_blind,
            ante: self.ante,
            acting: self.current_pos,
            dealer: self.dealer_pos,
            street: match self.state {
                TableState::Idle => StreetName::Idle,
                TableState::PreFlop => StreetName::PreFlop,
                TableState::Flop => StreetName::Flop,
                TableState::Turn => StreetName::Turn,
                TableState::River => StreetName::River,
            },
            your_seat,
            your_cards,
        }
    }

    fn is_betting(&self) -> bool {
        matches!(
            self.state,
            TableState::PreFlop | TableState::Flop | TableState::Turn | TableState::River
        )
    }

    fn remove_players(&mut self) {
        for slot in self.seats.iter_mut() {
            let remove = matches!(&*slot, Some(s) if s.left);
            if remove {
                *slot = None;
            }
        }
    }

    fn deal_cards(&mut self) {
        let mut deck = Deck::new();
        deck.shuffle();

        let mut seats_dealt = Vec::new();
        for (idx, seat) in self.seats.iter_mut().enumerate() {
            let Some(s) = seat else { continue; };
            if s.folded || s.left {
                continue;
            }
            s.cards[0] = deck.deal();
            s.cards[1] = deck.deal();
            if let [Some(first), Some(second)] = s.cards {
                self.hole_cards.push((SeatId(idx as u8), HoleCardEvent { cards: [first, second] }));
            }
            seats_dealt.push(SeatId(idx as u8));
        }
        self.deck = Some(deck);

        if !seats_dealt.is_empty() {
            self.events.push(GameEvent::Deal(DealEvent { seats_dealt }));
        }
    }

    fn start_hand(&mut self) {
        self.remove_players();

        if self.player_count() <= 1 {
            return;
        }

        self.dealer_pos = self.next_occupied_after(self.dealer_pos);

        for seat in self.seats.iter_mut().flatten() {
            seat.bet = 0;
            seat.acted = false;
            seat.folded = false;
            seat.cards = [None, None];
        }
        self.cards = [None, None, None, None, None];
        self.pot = 0;
        self.current_bet = 0;

        self.deal_cards();
        self.state = TableState::PreFlop;
        self.post_antes();

        // Heads-up the button posts the small blind and acts first before the
        // flop. With three or more players the blinds are the next two seats,
        // and action starts left of the big blind.
        let small_blind_seat = if self.player_count() == 2 {
            self.dealer_pos
        } else {
            self.dealer_pos.and_then(|dealer| self.next_occupied_after(Some(dealer)))
        };
        let big_blind_seat = small_blind_seat.and_then(|seat| self.next_occupied_after(Some(seat)));

        if let Some(seat) = small_blind_seat {
            self.post_blind(seat, self.small_blind, BlindKind::Small);
        }
        if let Some(seat) = big_blind_seat {
            self.post_blind(seat, self.big_blind, BlindKind::Big);
        }

        // The big blind sets the preflop price even if the blind itself was
        // clamped to a short stack.
        self.current_bet = self.big_blind;
        self.current_pos = big_blind_seat.and_then(|seat| self.next_active_after(seat));
        self.push_turn();
    }

    /// Every player dealt into the hand pays the ante straight into the pot.
    /// It is dead money: it neither counts as a street bet nor changes
    /// `current_bet`.
    fn post_antes(&mut self) {
        for idx in 0..self.seats.len() {
            let Some(seat) = self.seats[idx].as_mut() else { continue };
            if seat.left || seat.folded {
                continue;
            }
            let amount = self.ante.min(seat.bank);
            if amount == 0 {
                continue;
            }
            seat.bank -= amount;
            self.pot += amount;
            self.events.push(GameEvent::Ante(AnteEvent {
                seat: SeatId(idx as u8),
                amount: BetAmount(amount),
            }));
        }
    }

    /// A blind is a live bet: it goes into the pot and counts toward the
    /// player's current-round contribution, but it does not mark them as having
    /// acted.
    fn post_blind(&mut self, seat: u8, blind: u32, kind: BlindKind) {
        let amount = match self.seats.get_mut(seat as usize) {
            Some(Some(s)) if !s.left && !s.folded => {
                let amount = blind.min(s.bank);
                if amount > 0 {
                    s.bank -= amount;
                    s.bet += amount;
                }
                amount
            }
            _ => 0,
        };

        if amount == 0 {
            return;
        }
        self.pot += amount;
        self.events.push(GameEvent::Blind(BlindEvent {
            seat: SeatId(seat),
            amount: BetAmount(amount),
            kind,
        }));
    }

    fn is_active(&self, idx: usize) -> bool {
        matches!(self.seats.get(idx), Some(Some(s)) if !s.left && !s.folded)
    }

    fn can_act(&self, idx: usize) -> bool {
        matches!(self.seats.get(idx), Some(Some(s)) if !s.left && !s.folded && s.bank > 0)
    }

    fn players_in_hand(&self) -> usize {
        (0..self.seats.len()).filter(|&idx| self.is_active(idx)).count()
    }

    /// Next seat after `from` that is still in the hand and has chips to act with.
    fn next_active_after(&self, from: u8) -> Option<u8> {
        let n = self.seats.len();
        for step in 1..=n {
            let idx = (from as usize + step) % n;
            if self.can_act(idx) {
                return Some(idx as u8);
            }
        }
        None
    }

    fn next_occupied_after(&self, from: Option<u8>) -> Option<u8> {
        let n = self.seats.len();
        if n == 0 {
            return None;
        }
        let start = from.map(|f| f as usize).unwrap_or(n - 1);
        for step in 1..=n {
            let idx = (start + step) % n;
            if self.seats[idx].is_some() {
                return Some(idx as u8);
            }
        }
        None
    }

    fn round_complete(&self) -> bool {
        for idx in 0..self.seats.len() {
            if !self.is_active(idx) {
                continue;
            }
            let Some(s) = self.seats[idx].as_ref() else { continue; };
            if s.bank == 0 {
                continue; // all-in: cannot act any more
            }
            if !s.acted || s.bet < self.current_bet {
                return false;
            }
        }
        true
    }

    fn after_action(&mut self) {
        if self.players_in_hand() <= 1 {
            self.end_hand(false);
            return;
        }
        if self.round_complete() {
            self.end_street();
            return;
        }
        self.advance_turn();
    }

    fn advance_turn(&mut self) {
        let Some(current) = self.current_pos else { return; };
        let n = self.seats.len();
        for step in 1..=n {
            let idx = (current as usize + step) % n;
            if let Some(Some(s)) = self.seats.get(idx) {
                if !s.left && !s.folded && s.bank > 0 && (!s.acted || s.bet < self.current_bet) {
                    self.current_pos = Some(idx as u8);
                    self.push_turn();
                    return;
                }
            }
        }
        if self.round_complete() {
            self.end_street();
        }
    }

    fn end_street(&mut self) {
        for seat in self.seats.iter_mut().flatten() {
            seat.bet = 0;
            seat.acted = false;
        }
        self.current_bet = 0;

        match self.state {
            TableState::PreFlop => self.deal_board(3, TableState::Flop),
            TableState::Flop => self.deal_board(1, TableState::Turn),
            TableState::Turn => self.deal_board(1, TableState::River),
            _ => self.end_hand(true),
        }
    }

    fn deal_board(&mut self, count: usize, next: TableState) {
        let mut dealt = Vec::new();
        if let Some(deck) = &mut self.deck {
            for slot in self.cards.iter_mut() {
                if dealt.len() == count {
                    break;
                }
                if slot.is_none() {
                    if let Some(card) = deck.deal() {
                        *slot = Some(card);
                        dealt.push(card);
                    }
                }
            }
        }

        self.state = next;
        if !dealt.is_empty() {
            self.events.push(GameEvent::CommunityCards(CommunityCardsEvent { cards: dealt }));
        }

        self.current_pos = self.dealer_pos.and_then(|dealer| self.next_active_after(dealer));
        self.push_turn();
    }

    fn end_hand(&mut self, showdown: bool) {
        let winners: Vec<u8> = if showdown {
            self.showdown_winners()
        } else {
            (0..self.seats.len())
                .filter(|&idx| self.is_active(idx))
                .map(|idx| idx as u8)
                .collect()
        };

        let amount = self.pot;
        self.pot = 0;

        if !winners.is_empty() {
            // Publish the hands that reached the pot before the result, so a
            // client can show what won. Folded hands are never revealed.
            if showdown {
                self.events.push(GameEvent::Showdown(ShowdownEvent { hands: self.revealed_hands() }));
            }

            let share = amount / winners.len() as u32;
            let remainder = amount % winners.len() as u32;
            for (i, &seat) in winners.iter().enumerate() {
                if let Some(Some(s)) = self.seats.get_mut(seat as usize) {
                    s.bank += share + if i == 0 { remainder } else { 0 };
                    s.bet = 0;
                }
            }
            self.events.push(GameEvent::HandEnd(HandEndEvent {
                winners: winners.iter().map(|&seat| SeatId(seat)).collect(),
                amount,
                showdown,
            }));
        }

        self.state = TableState::Idle;
        self.current_pos = None;
        self.turn_ticks_left = 0;
    }

    fn showdown_winners(&self) -> Vec<u8> {
        let board: Vec<Card> = self.cards.iter().flatten().copied().collect();
        let mut best: Option<HandRank> = None;
        let mut winners: Vec<u8> = Vec::new();

        for idx in 0..self.seats.len() {
            if !self.is_active(idx) {
                continue;
            }
            let Some(seat) = self.seats[idx].as_ref() else { continue; };
            let mut cards = board.clone();
            cards.extend(seat.cards.iter().flatten().copied());
            let Some(rank) = best_hand(&cards) else { continue; };

            match best {
                None => {
                    best = Some(rank);
                    winners.clear();
                    winners.push(idx as u8);
                }
                Some(top) if rank > top => {
                    best = Some(rank);
                    winners.clear();
                    winners.push(idx as u8);
                }
                Some(top) if rank == top => winners.push(idx as u8),
                _ => {}
            }
        }

        winners
    }

    /// The two cards of every player still in the hand, in seat order.
    fn revealed_hands(&self) -> Vec<ShowdownHand> {
        (0..self.seats.len())
            .filter(|&idx| self.is_active(idx))
            .filter_map(|idx| {
                let seat = self.seats[idx].as_ref()?;
                match seat.cards {
                    [Some(first), Some(second)] => {
                        Some(ShowdownHand { seat: SeatId(idx as u8), cards: [first, second] })
                    }
                    _ => None,
                }
            })
            .collect()
    }

    fn push_turn(&mut self) {
        match self.current_pos {
            Some(pos) => {
                // Every new decision gives the actor a fresh clock.
                self.turn_ticks_left = Self::TURN_TICKS;
                self.events.push(GameEvent::Turn(TurnEvent { seat: SeatId(pos) }));
            }
            None => self.turn_ticks_left = 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::deck::Card;
    use crate::game_event::*;
    use crate::snapshot::StreetName;
    use crate::table::{BetAmount, SeatId, Table, TableState};

    fn two_handed() -> Table {
        let mut t = Table::new(8);
        t.sit(500);
        t.sit(500);
        t.drain_events();
        t.process();
        t.drain_events();
        t.drain_hole_cards();
        t
    }

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
        let s1 = t.sit(500);
        t.leave(0);
        assert_eq!(t.player_count(), 0);

    }

    #[test]
    fn sit_emits_one_event() {
        let mut t = Table::new(8);
        t.sit(500);
        assert_eq!(t.drain_events(), vec![GameEvent::Sit(SitEvent { seat: SeatId(0) })]);
        assert!(t.drain_events().is_empty());
    }

    #[test]
    fn leave_emits_event_once() {
        let mut t = Table::new(8);
        t.sit(500);
        t.drain_events();
        assert_eq!(t.leave(0), Some(500));
        assert_eq!(t.drain_events(), vec![GameEvent::Leave(LeaveEvent { seat: SeatId(0) })]);
        // Leaving an already-left seat reports the bank but is not a new event.
        assert_eq!(t.leave(0), Some(500));
        assert!(t.drain_events().is_empty());
    }

    #[test]
    fn process_emits_deal_event() {
        let mut t = Table::new(8);
        t.sit(500);
        t.sit(500);
        t.drain_events();
        t.process();
        assert_eq!(
            t.drain_events(),
            vec![
                GameEvent::Deal(DealEvent { seats_dealt: vec![SeatId(0), SeatId(1)] }),
                GameEvent::Ante(AnteEvent { seat: SeatId(0), amount: BetAmount(2) }),
                GameEvent::Ante(AnteEvent { seat: SeatId(1), amount: BetAmount(2) }),
                GameEvent::Blind(BlindEvent {
                    seat: SeatId(0),
                    amount: BetAmount(10),
                    kind: BlindKind::Small,
                }),
                GameEvent::Blind(BlindEvent {
                    seat: SeatId(1),
                    amount: BetAmount(20),
                    kind: BlindKind::Big,
                }),
                GameEvent::Turn(TurnEvent { seat: SeatId(0) }),
            ]
        );
    }

    #[test]
    fn process_does_not_deal_for_one_player() {
        let mut t = Table::new(8);
        t.sit(500);
        t.drain_events();
        t.process();
        assert!(t.drain_events().is_empty());
    }

    #[test]
    fn process_deals_private_hole_cards() {
        let mut t = Table::new(8);
        t.sit(500);
        t.sit(500);
        t.drain_events();
        t.process();

        // The public stream announces the deal and the forced bets without
        // revealing any card.
        let events = t.drain_events();
        assert!(events.contains(&GameEvent::Deal(DealEvent {
            seats_dealt: vec![SeatId(0), SeatId(1)],
        })));
        assert!(events.iter().filter(|event| matches!(event, GameEvent::Ante(_))).count() == 2);
        assert!(events.iter().filter(|event| matches!(event, GameEvent::Blind(_))).count() == 2);
        assert!(events.iter().any(|event| matches!(event, GameEvent::Turn(TurnEvent { seat: SeatId(0) }))));

        let hole_cards = t.drain_hole_cards();
        assert_eq!(hole_cards.len(), 2);
        assert_eq!(hole_cards[0].0, SeatId(0));
        assert_eq!(hole_cards[1].0, SeatId(1));

        let cards: Vec<Card> = hole_cards.iter().flat_map(|(_, h)| h.cards).collect();
        assert_eq!(cards.len(), 4);
        for i in 0..cards.len() {
            for j in (i + 1)..cards.len() {
                assert_ne!(cards[i], cards[j], "the same card was dealt twice");
            }
        }

        // Each deal is handed out once.
        assert!(t.drain_hole_cards().is_empty());
    }

    #[test]
    fn hole_cards_can_be_read_back_by_seat() {
        let mut t = Table::new(8);
        t.sit(500);
        t.sit(500);
        assert!(t.hole_cards(0).is_none(), "nothing has been dealt yet");

        t.process();

        let first = t.hole_cards(0).expect("seat 0 was dealt");
        let second = t.hole_cards(1).expect("seat 1 was dealt");
        assert_ne!(first.cards, second.cards);

        let dealt = t.drain_hole_cards();
        assert_eq!(dealt[0].1, first);
        assert_eq!(dealt[1].1, second);

        // Asking again is idempotent.
        assert_eq!(t.hole_cards(0), Some(first));
        assert!(t.hole_cards(2).is_none(), "empty seat has no cards");
        assert!(t.hole_cards(9).is_none(), "out of range seat has no cards");
    }

    #[test]
    fn heads_up_dealer_posts_the_small_blind_and_acts_first() {
        let mut t = two_handed();
        assert_eq!(t.state, TableState::PreFlop);
        assert_eq!(t.dealer_pos, Some(0));
        assert_eq!(t.current_pos, Some(0), "the heads-up dealer/small blind acts first");
        assert_eq!(t.pot, 34, "two antes plus the blinds are already in the pot");
        assert_eq!(t.current_bet, 20);
        assert_eq!(t.cards.iter().flatten().count(), 0);
        assert_eq!(t.seats[0].as_ref().unwrap().bank, 488, "ante 2 + small blind 10");
        assert_eq!(t.seats[0].as_ref().unwrap().bet, 10);
        assert_eq!(t.seats[1].as_ref().unwrap().bank, 478, "ante 2 + big blind 20");
        assert_eq!(t.seats[1].as_ref().unwrap().bet, 20);
    }

    #[test]
    fn three_handed_blinds_and_action_order() {
        let mut t = Table::new(8);
        t.sit(500);
        t.sit(500);
        t.sit(500);
        t.drain_events();
        t.process();

        assert_eq!(t.dealer_pos, Some(0));
        assert_eq!(t.pot, 36, "three antes of 2 plus blinds of 10 and 20");
        assert_eq!(t.current_bet, 20);
        assert_eq!(t.current_pos, Some(0), "action starts left of the big blind");
        assert_eq!(t.seats[0].as_ref().unwrap().bet, 0, "the dealer has no blind");
        assert_eq!(t.seats[0].as_ref().unwrap().bank, 498);
        assert_eq!(t.seats[1].as_ref().unwrap().bet, 10);
        assert_eq!(t.seats[1].as_ref().unwrap().bank, 488);
        assert_eq!(t.seats[2].as_ref().unwrap().bet, 20);
        assert_eq!(t.seats[2].as_ref().unwrap().bank, 478);

        let events = t.drain_events();
        assert!(events.contains(&GameEvent::Ante(AnteEvent { seat: SeatId(2), amount: BetAmount(2) })));
        assert!(events.contains(&GameEvent::Blind(BlindEvent {
            seat: SeatId(1),
            amount: BetAmount(10),
            kind: BlindKind::Small,
        })));
        assert!(events.contains(&GameEvent::Blind(BlindEvent {
            seat: SeatId(2),
            amount: BetAmount(20),
            kind: BlindKind::Big,
        })));
        assert_eq!(events.last(), Some(&GameEvent::Turn(TurnEvent { seat: SeatId(0) })));
    }

    #[test]
    fn with_stakes_configures_the_forced_bets() {
        let mut t = Table::with_stakes(8, 5, 10, 1);
        t.sit(500);
        t.sit(500);
        t.process();

        assert_eq!(t.current_bet, 10);
        assert_eq!(t.pot, 17, "two antes of 1 plus blinds of 5 and 10");
        let snapshot = t.snapshot(None);
        assert_eq!(snapshot.small_blind, 5);
        assert_eq!(snapshot.big_blind, 10);
        assert_eq!(snapshot.ante, 1);
    }

    #[test]
    fn short_blind_is_clamped_to_the_stack() {
        let mut t = Table::new(8);
        t.sit(5);
        t.sit(500);
        t.drain_events();
        t.process();

        // Seat 0 posts a 2 ante and then the 10 small blind, all-in for 3.
        assert_eq!(t.seats[0].as_ref().unwrap().bank, 0);
        assert_eq!(t.seats[0].as_ref().unwrap().bet, 3);
        assert_eq!(t.pot, 27, "antes 4 + short small blind 3 + big blind 20");
        assert_eq!(t.current_bet, 20);
        let events = t.drain_events();
        assert!(events.contains(&GameEvent::Blind(BlindEvent {
            seat: SeatId(0),
            amount: BetAmount(3),
            kind: BlindKind::Small,
        })));
    }

    #[test]
    fn betting_round_opens_the_flop() {
        let mut t = two_handed();
        t.bet(0, 10); // small blind calls the big blind
        t.check(1); // big blind checks its option

        assert_eq!(t.state, TableState::Flop);
        assert_eq!(t.pot, 44);
        assert_eq!(t.cards.iter().flatten().count(), 3);
        let events = t.drain_events();
        assert!(matches!(
            events.last(),
            Some(GameEvent::Turn(TurnEvent { seat: SeatId(1) }))
        ));
        assert!(events.iter().any(|e| matches!(e, GameEvent::CommunityCards(c) if c.cards.len() == 3)));
    }

    #[test]
    fn checking_through_opens_the_next_street() {
        let mut t = two_handed();
        t.bet(0, 10);
        t.check(1);
        t.drain_events();

        t.check(1);
        t.check(0);
        assert_eq!(t.state, TableState::Turn);
        assert_eq!(t.cards.iter().flatten().count(), 4);
    }

    #[test]
    fn turn_clock_checks_when_nothing_is_due() {
        let mut t = two_handed();
        t.bet(0, 10);
        t.check(1);
        t.drain_events();

        // The flop opens with seat 1 to act and nothing to call.
        for _ in 0..Table::TURN_TICKS {
            t.process();
        }

        assert_eq!(t.state, TableState::Flop, "one check is not the whole round");
        assert_eq!(t.current_pos, Some(0), "action moved to the other seat");
        let events = t.drain_events();
        assert!(events.contains(&GameEvent::Check(CheckEvent { seat: SeatId(1) })));
        assert!(!events.iter().any(|event| matches!(event, GameEvent::Fold(_))));

        // The next seat gets its own full clock.
        for _ in 0..Table::TURN_TICKS {
            t.process();
        }
        assert_eq!(t.state, TableState::Turn);
        assert_eq!(t.cards.iter().flatten().count(), 4);
    }

    #[test]
    fn turn_clock_folds_when_a_bet_is_due() {
        let mut t = two_handed();
        // The small blind is first to act and owes 10 more to call.
        for _ in 0..Table::TURN_TICKS {
            t.process();
        }

        assert_eq!(t.state, TableState::Idle);
        assert_eq!(t.pot, 0);
        let events = t.drain_events();
        assert_eq!(events[0], GameEvent::Fold(FoldEvent { seat: SeatId(0) }));
        let hand_end = events
            .iter()
            .find_map(|event| match event {
                GameEvent::HandEnd(hand_end) => Some(hand_end),
                _ => None,
            })
            .expect("the hand ended");
        assert_eq!(hand_end.winners, vec![SeatId(1)]);
        assert_eq!(hand_end.amount, 34, "the antes and both blinds are won");
        assert_eq!(t.seats[1].as_ref().unwrap().bank, 512, "the winner took the pot");
    }

    #[test]
    fn acting_restarts_the_turn_clock() {
        let mut t = two_handed();
        for _ in 0..Table::TURN_TICKS - 1 {
            t.process();
        }
        t.bet(0, 10); // call, moving the action to the big blind
        t.drain_events();

        for _ in 0..Table::TURN_TICKS - 1 {
            t.process();
        }
        // The call gave seat 1 a fresh clock, so it is not folded early.
        assert_eq!(t.state, TableState::PreFlop);
        assert_eq!(t.current_pos, Some(1));

        t.process();
        assert_eq!(t.state, TableState::Flop, "the reset clock still expires");
    }

    #[test]
    fn only_the_current_seat_can_act() {
        let mut t = two_handed();
        assert_eq!(t.bet(1, 10), None, "seat 1 is not to act");
        assert!(!t.check(1));
        assert!(!t.fold(1));
        assert_eq!(t.bet(0, 10), Some(478));
    }

    #[test]
    fn cannot_check_facing_a_bet() {
        let mut t = two_handed();
        assert!(!t.check(0), "there is a bet to call");
        assert_eq!(t.bet(0, 10), Some(478));
        assert!(t.check(1), "the big blind can check its option");
    }

    #[test]
    fn bet_is_clamped_to_the_stack() {
        let mut t = Table::new(8);
        t.sit(100);
        t.sit(500);
        t.drain_events();
        t.process();
        t.drain_events();

        // Seat 0 has 88 behind after the ante and small blind.
        assert_eq!(t.seats[0].as_ref().unwrap().bank, 88);
        assert_eq!(t.bet(0, 500), Some(0), "all-in for the rest of the stack");
        let events = t.drain_events();
        assert!(events.contains(&GameEvent::Bet(BetEvent {
            seat: SeatId(0),
            amount: BetAmount(88),
        })));
        assert_eq!(t.pot, 122);
    }

    #[test]
    fn fold_ends_the_hand_and_awards_the_pot() {
        let mut t = two_handed();
        t.drain_events();

        assert!(t.fold(0));
        assert_eq!(t.state, TableState::Idle);
        assert_eq!(t.pot, 0);

        let events = t.drain_events();
        assert!(events.contains(&GameEvent::Fold(FoldEvent { seat: SeatId(0) })));
        let hand_end = events
            .iter()
            .find_map(|e| match e {
                GameEvent::HandEnd(h) => Some(h),
                _ => None,
            })
            .expect("the hand ended");
        assert!(!hand_end.showdown);
        assert!(
            !events.iter().any(|event| matches!(event, GameEvent::Showdown(_))),
            "hands folded before the showdown stay private"
        );
        assert_eq!(hand_end.amount, 34);
        assert_eq!(hand_end.winners, vec![SeatId(1)]);
        assert_eq!(t.seats[1].as_ref().unwrap().bank, 512);
    }

    #[test]
    fn showdown_awards_the_pot() {
        let mut t = two_handed();
        t.bet(0, 10);
        t.check(1);

        t.check(1);
        t.check(0);
        t.check(1);
        t.check(0);
        t.check(1);
        t.check(0);

        assert_eq!(t.state, TableState::Idle);
        assert_eq!(t.pot, 0);
        assert_eq!(t.cards.iter().flatten().count(), 5);

        let events = t.drain_events();
        let hand_end = events
            .iter()
            .find_map(|e| match e {
                GameEvent::HandEnd(h) => Some(h),
                _ => None,
            })
            .expect("the hand ended");
        assert!(hand_end.showdown);
        assert_eq!(hand_end.amount, 44);
        assert!(!hand_end.winners.is_empty());

        let chips: u32 = t.seats.iter().flatten().map(|s| s.bank).sum();
        assert_eq!(chips, 1000, "chips are conserved");
    }

    #[test]
    fn showdown_publishes_the_hands_that_reached_it() {
        let mut t = two_handed();
        let seat_0 = t.hole_cards(0).expect("seat 0 was dealt").cards;
        let seat_1 = t.hole_cards(1).expect("seat 1 was dealt").cards;

        t.bet(0, 10);
        t.check(1);
        t.check(1);
        t.check(0);
        t.check(1);
        t.check(0);
        t.check(1);
        t.check(0);

        let events = t.drain_events();
        let showdown = events
            .iter()
            .find_map(|event| match event {
                GameEvent::Showdown(showdown) => Some(showdown),
                _ => None,
            })
            .expect("the hand published its cards");

        assert_eq!(showdown.hands.len(), 2);
        assert!(showdown.hands.contains(&ShowdownHand { seat: SeatId(0), cards: seat_0 }));
        assert!(showdown.hands.contains(&ShowdownHand { seat: SeatId(1), cards: seat_1 }));

        // The reveal comes before the result it explains.
        let showdown_at = events.iter().position(|event| matches!(event, GameEvent::Showdown(_))).unwrap();
        let hand_end_at = events.iter().position(|event| matches!(event, GameEvent::HandEnd(_))).unwrap();
        assert!(showdown_at < hand_end_at);
    }

    #[test]
    fn all_in_runs_out_the_board() {
        let mut t = two_handed();
        t.bet(0, 500);
        t.bet(1, 500);
        // Nobody can act any more, so the remaining streets are dealt and the
        // hand goes to showdown on the next tick.
        t.process();

        assert_eq!(t.state, TableState::Idle);
        assert_eq!(t.cards.iter().flatten().count(), 5);

        let events = t.drain_events();
        let hand_end = events
            .iter()
            .find_map(|e| match e {
                GameEvent::HandEnd(h) => Some(h),
                _ => None,
            })
            .expect("the hand ended");
        assert!(hand_end.showdown);
        assert_eq!(hand_end.amount, 1000);
        assert!(events.iter().any(|event| matches!(event, GameEvent::Showdown(s) if s.hands.len() == 2)));
        let chips: u32 = t.seats.iter().flatten().map(|s| s.bank).sum();
        assert_eq!(chips, 1000);
    }

    #[test]
    fn dealer_moves_to_the_next_seat_each_hand() {
        let mut t = two_handed();
        t.fold(0);
        assert_eq!(t.state, TableState::Idle);

        t.process();
        assert_eq!(t.dealer_pos, Some(1));
        assert_eq!(t.current_pos, Some(1), "the new heads-up dealer acts first");
    }

    #[test]
    fn sitting_mid_hand_waits_for_the_next_one() {
        let mut t = two_handed();
        assert_eq!(t.sit(500), Some(2));
        assert!(t.seats[2].as_ref().unwrap().folded);
        assert_eq!(t.player_count(), 3);

        t.fold(0);
        t.process();
        assert!(!t.seats[2].as_ref().unwrap().folded, "dealt into the next hand");
    }

    #[test]
    fn snapshot_shows_public_state_and_only_the_viewers_cards() {
        let mut t = two_handed();
        t.bet(0, 10); // the small blind calls, action moves to the big blind

        let for_seat_1 = t.snapshot(Some(1));
        assert_eq!(for_seat_1.your_seat, Some(1));
        assert_eq!(for_seat_1.your_cards, t.hole_cards(1).map(|hand| hand.cards));
        assert_ne!(for_seat_1.your_cards, t.hole_cards(0).map(|hand| hand.cards));
        assert_eq!(for_seat_1.street, StreetName::PreFlop);
        assert_eq!(for_seat_1.acting, Some(1));
        assert_eq!(for_seat_1.dealer, Some(0));
        assert_eq!(for_seat_1.pot, 44, "antes, blinds and the call");
        assert_eq!(for_seat_1.current_bet, 20);
        assert_eq!(for_seat_1.small_blind, 10);
        assert_eq!(for_seat_1.big_blind, 20);
        assert_eq!(for_seat_1.ante, 2);
        assert!(for_seat_1.board.is_empty());
        assert_eq!(for_seat_1.seats.len(), 8);
        assert_eq!(for_seat_1.seats.iter().filter(|seat| seat.occupied).count(), 2);

        // The big blind's stack and blind are public.
        assert_eq!(for_seat_1.seats[1].stack, 478);
        assert_eq!(for_seat_1.seats[1].bet, 20);

        // A spectator view never carries a hand.
        let spectator = t.snapshot(None);
        assert_eq!(spectator.your_seat, None);
        assert_eq!(spectator.your_cards, None);
    }

    #[test]
    fn snapshot_after_a_hand_has_no_actor() {
        let mut t = two_handed();
        t.fold(0);

        let snapshot = t.snapshot(Some(1));
        assert_eq!(snapshot.street, StreetName::Idle);
        assert_eq!(snapshot.acting, None);
        assert_eq!(snapshot.pot, 0);
        assert!(snapshot.seats[0].folded);
        assert_eq!(snapshot.seats[1].stack, 512, "the winner took the pot");
    }

    #[test]
    fn snapshot_does_not_hand_cards_to_a_departed_seat() {
        let mut t = two_handed();
        t.leave(0);

        let snapshot = t.snapshot(Some(0));
        assert_eq!(snapshot.your_seat, None);
        assert_eq!(snapshot.your_cards, None);
        assert!(!snapshot.seats[0].occupied);
    }
}
