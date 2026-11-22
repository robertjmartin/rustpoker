use rand::seq::SliceRandom;
use rand::thread_rng;
use serde::Serialize;
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT_DECK_ID: AtomicU32 = AtomicU32::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeckId(u32);

impl DeckId {
    fn next() -> Self {
        DeckId(NEXT_DECK_ID.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Rank {
    Two = 2,
    Three = 3,
    Four = 4,
    Five = 5,
    Six = 6,
    Seven = 7,
    Eight = 8,
    Nine = 9,
    Ten = 10,
    Jack = 11,
    Queen = 12,
    King = 13,
    Ace = 14,
}

impl Rank {
    const ALL: [Rank; 13] = [Rank::Two, Rank::Three, Rank::Four, Rank::Five, Rank::Six, Rank::Seven, Rank::Eight, Rank::Nine, Rank::Ten, Rank::Jack, Rank::Queen, Rank::King, Rank::Ace];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Suit {
    Hearts,
    Diamonds,
    Spades,
    Clubs
}

impl Suit {
    const ALL: [Suit; 4] = [Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Card {
    #[serde(skip_serializing)]
    deck_id: DeckId,
    suit: Suit,
    rank: Rank,
}

impl Card {
    /// Build a card directly. Cards dealt from a `Deck` also carry the deck
    /// they came from; this is for tests and for clients of the evaluator.
    pub fn new(rank: Rank, suit: Suit) -> Card {
        Card { rank, suit, deck_id: DeckId::next() }
    }

    pub fn get_value(&self) -> (Rank, Suit) {
        (self.rank, self.suit)
    }
}

pub struct Deck {
    cards: Vec<Card>,
    deck_id: DeckId, 
}

impl Deck {
    pub fn new() -> Self {
        let deck_id = DeckId::next();
        let mut cards = Vec::with_capacity(52);
        for suit in Suit::ALL {
            for rank in Rank::ALL {
                cards.push( Card {rank, suit, deck_id});
            }
        }
        Deck {
            cards,
            deck_id,
        }
    }

    pub fn shuffle(&mut self) {
        let mut rng = thread_rng();
        self.cards.shuffle(&mut rng);
    }

    pub fn count(&self) -> usize {
        self.cards.len()
    }

    pub fn deal(&mut self) -> Option<Card> {
        self.cards.pop()
    }
}

