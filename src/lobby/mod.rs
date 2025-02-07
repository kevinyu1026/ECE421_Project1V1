//!
use std::string;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use sqlx::SqlitePool;
use crate::Card;
use crate::Deck;
use warp::ws::Message;

// Lobby attribute definitions
pub const MAX_PLAYER_COUNT: i32 = 5;
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

// Player state definitions
pub const READY:i32 = 0;
const FOLDED: i32 = 1;
const ALL_IN: i32 = 2;
const CHECKED: i32 = 3;
const CALLED: i32 = 4;
const SPECTATING: i32 = 5;

// Define Player struct
#[derive(Clone)]
pub struct Player {
    pub name: String,
    pub id: String,
    pub hand: Vec<i32>,
    pub wallet: i32,
    pub tx: mpsc::UnboundedSender<Message>,
    pub state: i32,
    pub current_bet: i32,
    pub dealer: bool,
    pub ready: bool,
    pub games_played: i32,
    pub games_won: i32,
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

    fn game_state_machine(){

        // invoke all methods to conduct a 5 card draw game
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
        // iterate through players and check if all players are ready (2 or more players)
        // if all players are ready, start the game
        // game_state_machine();
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
        // if round == ANTE_ROUND{
        //     self.first_betting_player = (self.first_betting_player + 1) % self.current_player_count;
        // }
        // let mut players = self.players.lock().await;
        // self.current_max_bet = 0;
        // for player in players.iter_mut() {
        //     player.current_bet = 0;
        // }
        // let mut current_player_index = self.first_betting_player;
        // let mut current_bet = 0;
        // let mut players_remaining = self.current_player_count;
        // while players_remaining > 1 {
        //     let player = &mut players[current_player_index as usize];
    
        //     let mut message = format!("{{\"action\": \"bet\", \"current_bet\": {}, \"pot\": {}}}", current_bet, self.pot);
        //     let _ = player.tx.send(Message::text(message));
        //     let response = player.tx.recv().await.unwrap();
        //     let response = response.to_str().unwrap();
        //     let response: serde_json::Value = serde_json::from_str(response).unwrap();
        //     let action = response["action"].as_str().unwrap();
        //     if action == "fold" {
        //         players_remaining -= 1;
        //     } else if action == "bet" {
        //         let bet = response["bet"].as_i64().unwrap() as i32;
        //         player.cash -= bet;
        //         player.current_bet = bet;
        //         self.pot += bet;
        //         current_bet = bet;
        //     }
        // }
    }

    async fn showdown(&self) {
        let players: Vec<Player> = self.players.lock().await.to_vec();
        let mut winning_players: Vec<Player> = Vec::new(); // keeps track of winning players at the end, accounting for draws
        let mut winning_hand = (0, 0); // keeps track of current highest hand, could change when incrementing between players
        for player in players {
            if player.state == FOLDED || player.state == SPECTATING {continue};
            let player_hand = player.hand.clone();
            let player_hand_type = get_hand_type(&player_hand);
            if player_hand_type.0 > winning_hand.0 || (player_hand_type.0 == winning_hand.0 && player_hand_type.1 > winning_hand.1) {
                winning_hand = player_hand_type;
                winning_players.clear();
                winning_players.push(player);
            } else if player_hand_type.0 == winning_hand.0 && player_hand_type.1 == winning_hand.1 {
                winning_players.push(player);
            }
        }
        let winning_player_count = winning_players.len();
        let pot_share = self.pot / winning_player_count as i32;
        for player in winning_players {
            let mut player = player.clone();
            player.wallet += pot_share;
        }
    }
    async fn player_leave(&self, username: &str) {
        // let mut players = self.players.lock().await;
        // if let Some(player) = players.iter_mut().find(|p| p.name == username) {
        //     player.state = SPECTATING;
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