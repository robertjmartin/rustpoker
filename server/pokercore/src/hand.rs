use crate::deck::Card;

/// A comparable score for a five-card hand. `category` is 8 for a straight
/// flush down to 0 for high card; `tiebreak` holds the ranks that decide ties
/// in poker order (high to low), padded with zeros.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct HandRank {
    category: u8,
    tiebreak: [u8; 5],
}

impl HandRank {
    pub fn category(&self) -> u8 {
        self.category
    }
}

/// The best five-card hand from the cards given (2 hole cards + 5 board cards).
pub fn best_hand(cards: &[Card]) -> Option<HandRank> {
    let n = cards.len();
    if n < 5 {
        return None;
    }

    let mut best: Option<HandRank> = None;
    for a in 0..n {
        for b in (a + 1)..n {
            for c in (b + 1)..n {
                for d in (c + 1)..n {
                    for e in (d + 1)..n {
                        let hand = [cards[a], cards[b], cards[c], cards[d], cards[e]];
                        let rank = rank_five(&hand);
                        if best.map_or(true, |current| rank > current) {
                            best = Some(rank);
                        }
                    }
                }
            }
        }
    }
    best
}

fn rank_five(cards: &[Card; 5]) -> HandRank {
    let mut ranks = [0u8; 5];
    for (i, card) in cards.iter().enumerate() {
        let (rank, _) = card.get_value();
        ranks[i] = rank as u8;
    }
    ranks.sort_unstable_by(|a, b| b.cmp(a));

    let (_, suit0) = cards[0].get_value();
    let flush = cards.iter().all(|c| c.get_value().1 == suit0);
    let straight = straight_high(&ranks);

    // (multiplicity, rank) pairs, most frequent first, then highest rank.
    let mut groups: Vec<(u8, u8)> = Vec::new();
    for &rank in &ranks {
        match groups.iter_mut().find(|(_, group_rank)| *group_rank == rank) {
            Some(group) => group.0 += 1,
            None => groups.push((1, rank)),
        }
    }
    groups.sort_unstable_by(|a, b| b.cmp(a));

    if let (true, Some(high)) = (flush, straight) {
        return HandRank { category: 8, tiebreak: [high, 0, 0, 0, 0] };
    }
    if groups[0].0 == 4 {
        return HandRank { category: 7, tiebreak: [groups[0].1, groups[1].1, 0, 0, 0] };
    }
    if groups[0].0 == 3 && groups[1].0 == 2 {
        return HandRank { category: 6, tiebreak: [groups[0].1, groups[1].1, 0, 0, 0] };
    }
    if flush {
        return HandRank { category: 5, tiebreak: ranks };
    }
    if let Some(high) = straight {
        return HandRank { category: 4, tiebreak: [high, 0, 0, 0, 0] };
    }
    if groups[0].0 == 3 {
        return HandRank { category: 3, tiebreak: [groups[0].1, groups[1].1, groups[2].1, 0, 0] };
    }
    if groups[0].0 == 2 && groups[1].0 == 2 {
        return HandRank { category: 2, tiebreak: [groups[0].1, groups[1].1, groups[2].1, 0, 0] };
    }
    if groups[0].0 == 2 {
        return HandRank {
            category: 1,
            tiebreak: [groups[0].1, groups[1].1, groups[2].1, groups[3].1, 0],
        };
    }
    HandRank { category: 0, tiebreak: ranks }
}

fn straight_high(ranks: &[u8; 5]) -> Option<u8> {
    // `ranks` is sorted high to low.
    let distinct = ranks[0] != ranks[1]
        && ranks[1] != ranks[2]
        && ranks[2] != ranks[3]
        && ranks[3] != ranks[4];
    if !distinct {
        return None;
    }
    if ranks[0] - ranks[4] == 4 {
        return Some(ranks[0]);
    }
    // A-2-3-4-5: the ace plays low.
    if *ranks == [14, 5, 4, 3, 2] {
        return Some(5);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::{Card, Rank, Suit};

    fn c(rank: Rank, suit: Suit) -> Card {
        Card::new(rank, suit)
    }

    #[test]
    fn recognises_every_category() {
        let straight_flush = [
            c(Rank::Ace, Suit::Spades),
            c(Rank::King, Suit::Spades),
            c(Rank::Queen, Suit::Spades),
            c(Rank::Jack, Suit::Spades),
            c(Rank::Ten, Suit::Spades),
        ];
        assert_eq!(rank_five(&straight_flush).category(), 8);

        let quads = [
            c(Rank::Nine, Suit::Spades),
            c(Rank::Nine, Suit::Hearts),
            c(Rank::Nine, Suit::Clubs),
            c(Rank::Nine, Suit::Diamonds),
            c(Rank::Two, Suit::Spades),
        ];
        assert_eq!(rank_five(&quads).category(), 7);

        let full_house = [
            c(Rank::King, Suit::Spades),
            c(Rank::King, Suit::Hearts),
            c(Rank::King, Suit::Clubs),
            c(Rank::Four, Suit::Diamonds),
            c(Rank::Four, Suit::Spades),
        ];
        assert_eq!(rank_five(&full_house).category(), 6);

        let flush = [
            c(Rank::Ace, Suit::Hearts),
            c(Rank::Nine, Suit::Hearts),
            c(Rank::Seven, Suit::Hearts),
            c(Rank::Four, Suit::Hearts),
            c(Rank::Two, Suit::Hearts),
        ];
        assert_eq!(rank_five(&flush).category(), 5);

        let straight = [
            c(Rank::Nine, Suit::Spades),
            c(Rank::Eight, Suit::Hearts),
            c(Rank::Seven, Suit::Clubs),
            c(Rank::Six, Suit::Diamonds),
            c(Rank::Five, Suit::Spades),
        ];
        assert_eq!(rank_five(&straight).category(), 4);

        let trips = [
            c(Rank::Queen, Suit::Spades),
            c(Rank::Queen, Suit::Hearts),
            c(Rank::Queen, Suit::Clubs),
            c(Rank::Nine, Suit::Diamonds),
            c(Rank::Two, Suit::Spades),
        ];
        assert_eq!(rank_five(&trips).category(), 3);

        let two_pair = [
            c(Rank::Jack, Suit::Spades),
            c(Rank::Jack, Suit::Hearts),
            c(Rank::Four, Suit::Clubs),
            c(Rank::Four, Suit::Diamonds),
            c(Rank::Ace, Suit::Spades),
        ];
        assert_eq!(rank_five(&two_pair).category(), 2);

        let pair = [
            c(Rank::Ten, Suit::Spades),
            c(Rank::Ten, Suit::Hearts),
            c(Rank::Eight, Suit::Clubs),
            c(Rank::Five, Suit::Diamonds),
            c(Rank::Two, Suit::Spades),
        ];
        assert_eq!(rank_five(&pair).category(), 1);

        let high_card = [
            c(Rank::Ace, Suit::Spades),
            c(Rank::Jack, Suit::Hearts),
            c(Rank::Eight, Suit::Clubs),
            c(Rank::Five, Suit::Diamonds),
            c(Rank::Two, Suit::Spades),
        ];
        assert_eq!(rank_five(&high_card).category(), 0);
    }

    #[test]
    fn wheel_is_a_straight_and_loses_to_six_high() {
        let wheel = [
            c(Rank::Ace, Suit::Spades),
            c(Rank::Two, Suit::Hearts),
            c(Rank::Three, Suit::Clubs),
            c(Rank::Four, Suit::Diamonds),
            c(Rank::Five, Suit::Spades),
        ];
        let six_high = [
            c(Rank::Six, Suit::Spades),
            c(Rank::Two, Suit::Hearts),
            c(Rank::Three, Suit::Clubs),
            c(Rank::Four, Suit::Diamonds),
            c(Rank::Five, Suit::Spades),
        ];
        assert_eq!(rank_five(&wheel).category(), 4);
        assert!(rank_five(&six_high) > rank_five(&wheel));
    }

    #[test]
    fn kickers_break_ties() {
        let ace_kicker = [
            c(Rank::King, Suit::Spades),
            c(Rank::King, Suit::Hearts),
            c(Rank::Ace, Suit::Clubs),
            c(Rank::Nine, Suit::Diamonds),
            c(Rank::Two, Suit::Spades),
        ];
        let queen_kicker = [
            c(Rank::King, Suit::Spades),
            c(Rank::King, Suit::Hearts),
            c(Rank::Queen, Suit::Clubs),
            c(Rank::Nine, Suit::Diamonds),
            c(Rank::Two, Suit::Spades),
        ];
        assert!(rank_five(&ace_kicker) > rank_five(&queen_kicker));
    }

    #[test]
    fn best_of_seven_picks_the_straight_flush() {
        let cards = [
            c(Rank::Ace, Suit::Hearts),
            c(Rank::King, Suit::Hearts),
            c(Rank::Queen, Suit::Hearts),
            c(Rank::Ten, Suit::Hearts),
            c(Rank::Three, Suit::Diamonds),
            c(Rank::Jack, Suit::Hearts),
            c(Rank::Seven, Suit::Spades),
        ];
        let best = best_hand(&cards).expect("seven cards score");
        assert_eq!(best.category(), 8, "the jack completes the royal flush");
    }

    #[test]
    fn needs_five_cards() {
        assert!(best_hand(&[c(Rank::Ace, Suit::Hearts)]).is_none());
    }
}
