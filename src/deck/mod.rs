use rand::seq::SliceRandom;
use rand::prelude::*;
use rand::rng;

#[derive(Debug, Clone)]
pub struct Card {
    pub suit: char, // 'H', 'D', 'C', 'S'
    pub rank: String, // "2" to "10", "J", "Q", "K", "A"
}

pub struct Deck {
    // next_card_index: i32,
    // cards: Vec<i32>,
    cards: Vec<Card>,
    original_deck: Vec<Card>, // Store the original 52-card deck for resetting
}

impl Deck {
    // Create a new 52-card deck
    // pub fn new() -> Deck{
    //     Deck{next_card_index: 0, cards: (0..52).collect()}
    // }
    pub fn new() -> Self {
        let suits = ['H', 'D', 'C', 'S']; // Hearts, Diamonds, Clubs, Spades
        let ranks = vec![
            "2", "3", "4", "5", "6", "7", "8", "9", "10", "J", "Q", "K", "A",
        ];

        let mut cards = Vec::new();
        for &suit in &suits {
            for rank in &ranks {
                cards.push(Card {
                    suit,
                    rank: rank.to_string(),
                });
            }
        }

        // Store the original deck for resetting
        let original_deck = cards.clone();

        Self { cards, original_deck }
    }

    // Shuffle the deck
    // pub fn shuffle(&mut self){
    //     self.cards.shuffle(&mut rng());
    // }
    pub fn shuffle(&mut self) {
        // let mut rng = thread_rng();
        self.cards.shuffle(&mut rng());
    }



    // Deal one card from the top of the deck
    pub fn deal_one_card(&mut self) -> Option<Card> {
        self.cards.pop()
    }
    // pub fn deal(&mut self) -> i32{
    //     let card = self.cards[self.next_card_index as usize];
    //     self.next_card_index += 1;
    //     card
    // }


    // Reset the deck back to the original 52 cards
    // pub fn reset(&mut self){
    //     self.next_card_index = 0;
    //     self.shuffle();
    // }
    pub fn reset(&mut self) {
        self.cards = self.original_deck.clone();
        self.shuffle();
    }

    // Display the remaining cards in the deck
    pub fn display_remaining_cards(&self) {
        for card in &self.cards {
            println!("{}{}", card.rank, card.suit);
        }
    }
}




//usage--------------------
// fn main() {
//     let mut new_deck = Deck::new();
//     new_deck.shuffle();
//     for _ in 0..52{
//         println!("{:?}", new_deck.deal());
//     }
// }


