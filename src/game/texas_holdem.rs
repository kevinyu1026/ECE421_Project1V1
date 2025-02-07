//In this file im going to create functions for 
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use sqlx::SqlitePool;
use crate::Card;
use crate::Deck;
use warp::ws::Message;

const MAX_PLAYER_COUNT: i32 = 5;
const EMPTY: i32 = 0;
const START_OF_ROUND: i32 = 1;
const ANTE: i32 = 2;
const DEAL_CARDS: i32 = 3;
const FIRST_BETTING_ROUND: i32 = 4;
const DRAW: i32 = 5;
const SECOND_BETTING_ROUND: i32 = 6;
const SHOWDOWN: i32 = 7;
const END_OF_ROUND: i32 = 8;
const UPDATE_DB: i32 = 9;
const ANTE_ROUND: i32 = 0;
const FIRST_ROUND: i32 = 1;
const SECOND_ROUND: i32 = 2;

// Define Player struct
pub struct Player {
    pub name: String,
    pub id: String,
    pub hand: Vec<i32>,
    pub cash: i32,
    pub tx: mpsc::UnboundedSender<Message>,
    pub state: String,
    pub current_bet: i32,
    pub dealer: bool,
    pub ready: bool,
}


pub struct Lobby {
    pub players: Mutex<Vec<Player>>, // Store full Player objects
    pub game_db: SqlitePool,
    deck: Deck,
    current_max_bet: i32,
    pot: i32,
    current_player_count: i32,
    game_state: i32,
    first_betting_player: i32,
}

impl Lobby {
    pub async fn new() -> Self {
        Lobby {
            deck: Deck::new(),
            players: Mutex::new(Vec::new()),
            current_max_bet: 0,
            current_player_count: 0,
            pot: 0,
            game_state: EMPTY,
            first_betting_player: 0,
            game_db: SqlitePool::connect("sqlite://poker.db").await.unwrap(),
        }
    }

    // Add player to the lobby
    pub async fn add_player(&self, player: Player) {
        let mut players = self.players.lock().await;
        players.push(player);
    }

    // Remove player from the lobby
    pub async fn remove_player(&self, username: &str) {
        let mut players = self.players.lock().await;
        players.retain(|p| p.name != username);
    }

    // Broadcast a message to all players
    pub async fn broadcast(&self, message: String) {
        let players = self.players.lock().await;
        for player in players.iter() {
            let _ = player.tx.send(Message::text(message.clone()));
        }
    }

    pub async fn ready_up(&self, username: &str) {
        let mut players = self.players.lock().await;
        if let Some(player) = players.iter_mut().find(|p| p.name == username) {
            player.ready = true;
        }
    }

    async fn deal_cards(&mut self) {
        let mut players = self.players.lock().await;
        for _ in 0..5 {
            for player in players.iter_mut() {
                player.hand.push(self.deck.deal());
            }

        }
    }

    async fn betting_round(&mut self, round: i32) {
        if round == ANTE_ROUND{
            self.first_betting_player = (self.first_betting_player + 1) % self.current_player_count;
        }
        let mut players = self.players.lock().await;
        self.current_max_bet = 0;
        for player in players.iter_mut() {
            player.current_bet = 0;
        }
        let mut current_player_index = self.first_betting_player;
        let mut current_bet = 0;
        let mut players_remaining = self.current_player_count;

        while players_remaining > 1 {
            let player = &mut players[current_player_index as usize];
    
            let mut message = format!("{{\"action\": \"bet\", \"current_bet\": {}, \"pot\": {}}}", current_bet, self.pot);
            
            let _ = player.tx.send(Message::text(message));
            if let Some(Ok(response_msg)) = player.tx.recv().await {
            if let Ok(response_str) = response_msg.to_str() {
                if let Ok(response) = serde_json::from_str::<serde_json::Value>(response_str) {
                    if let Some(action) = response["action"].as_str() {
                        match action {
                            "fold" => {
                                players_remaining -= 1;
                            }
                            "call" => {
                                let call_amount = self.current_max_bet - player.current_bet;
                                if call_amount > player.cash {
                                    player.cash = 0;
                                    player.current_bet += call_amount;
                                    self.pot += call_amount;
                                } else {
                                    player.cash -= call_amount;
                                    player.current_bet += call_amount;
                                    self.pot += call_amount;
                                }
                            }
                            "raise" => {
                                if let Some(bet) = response["bet"].as_i64() {
                                    let bet = bet as i32;
                                    if bet > player.cash {
                                        player.tx.send(Message::text("Invalid bet: not enough cash.")).ok();
                                    } else {
                                        player.cash -= bet;
                                        player.current_bet += bet;
                                        self.pot += bet;
                                        self.current_max_bet = player.current_bet;
                                    }
                                }
                            }
                            "all-in" => {
                                let all_in_amount = player.cash;
                                player.current_bet += all_in_amount;
                                self.pot += all_in_amount;
                                player.cash = 0;
                                self.current_max_bet = player.current_bet;
                            }
                            "check" => {
                                if player.current_bet < self.current_max_bet {
                                    player.tx.send(Message::text("Invalid move: You can't check, there's a bet to call.")).ok();
                                }
                            }
                            _ => {
                                player.tx.send(Message::text("Invalid action.")).ok();
                            }
                        }
                    }
                }
            }
        }

        // Move to next player
        current_player_index = (current_player_index + 1) % self.current_player_count;
    }
}
    

    async fn showdown(&self) {
        // let mut players = self.players.lock().await;
        // let mut winning_player = &players[0];
        // for player in players.iter() {
        //     if player.current_bet > winning_player.current_bet {
        //         winning_player = player;
        //     }
        // }
        // for player in players {
        //     if player.current_bet == winning_player.current_bet {
        //         player.cash += self.pot;
        //     }
        // }
    }
}


fn get_hand_type(hand: &[i32]) -> (i32, i32) {
    assert!(hand.len() == 5);

    let mut ranks: Vec<i32> = hand.iter().map(|&card| if card != 0 { card % 13 } else { 13 }).collect();
    ranks.sort();

    let suits: Vec<i32> = hand.iter().map(|&card| card / 13).collect();

    // Check for flush
    let flush = suits.iter().all(|&suit| suit == suits[0]);

    // Check for straight
    let straight = (1..5).all(|i| ranks[i] == ranks[i - 1] + 1);

    if flush && straight {
        return (8, ranks[4]);
    }

    // Check for four of a kind
    for i in 0..2 {
        if ranks[i] == ranks[i + 1] && ranks[i] == ranks[i + 2] && ranks[i] == ranks[i + 3] {
            return (7, ranks[i]);
        }
    }

    // Check for full house
    if (ranks[0] == ranks[1] && ranks[3] == ranks[4]) && (ranks[2] == ranks[0] || ranks[2] == ranks[4]) {
        return (6, ranks[4]);
    }

    if flush {
        return (5, ranks[4]);
    }

    if straight {
        return (4, ranks[4]);
    }

    // Check for three of a kind
    for i in 0..3 {
        if ranks[i] == ranks[i + 1] && ranks[i] == ranks[i + 2] {
            return (3, ranks[i]);
        }
    }

    // Check for two pair
    if ranks[0] == ranks[1] && ranks[2] == ranks[3] {
        return (3, ranks[4]);
    } else if ranks[0] == ranks[1] && ranks[3] == ranks[4] {
        return (3, ranks[2]);
    } else if ranks[1] == ranks[2] && ranks[3] == ranks[4] {
        return (3, ranks[0]);
    }

    // Check for one pair
    for i in 0..4 {
        if ranks[i] == ranks[i + 1] {
            if i == 3 {
                return (2, ranks[2]);
            } else {
                return (2, ranks[4]);
            }
        }
    }

    // High card
    (1, ranks[4])
}

fn main() {
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).expect("Failed to read line");
    let hand: Vec<i32> = input.trim().split_whitespace().map(|s| s.parse().unwrap()).collect();
    let hand_type = get_hand_type(&hand);
    println!("{:?}", hand_type);
}
