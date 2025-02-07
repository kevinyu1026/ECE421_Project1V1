//!
use std::string;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use sqlx::SqlitePool;
use crate::Deck;
use warp::{filters::ws::WebSocket, ws::Message};
use super::*;

// Lobby attribute definitions
pub const MAX_PLAYER_COUNT: i32 = 5;
const EMPTY: i32 = -1;
pub const JOINABLE: i32 = 0;
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
pub const IN_LOBBY: i32 = 5;
pub const IN_SERVER:i32 = 6;

// Method return defintions
pub const SUCCESS: i32 = 100;
pub const FAILED: i32 = 101;
pub const SERVER_FULL: i32 = 102;

// Define Player struct
#[derive(Clone)]
pub struct Player {
    pub name: String,
    pub id: String,
    pub hand: Vec<i32>,
    pub wallet: i32,
    pub tx: mpsc::UnboundedSender<Message>,
    pub rx: Arc<Mutex<SplitStream<warp::ws::WebSocket>>>,
    pub state: i32,
    pub current_bet: i32,
    pub dealer: bool,
    pub ready: bool,
    pub games_played: i32,
    pub games_won: i32,
    pub lobby: Lobby,
}

impl Player {
    pub async fn remove_player(&self, mut server_lobby: Lobby, db: Arc<Database>) {
        db.update_player_stats(&self).await.unwrap();

        server_lobby.remove_player(server_lobby.clone(), self.name.clone()).await;
        server_lobby.broadcast(
            format!("{} has left the server.", self.name)
        ).await;
    }

    pub async fn get_player_input(&self, username_id: String) -> String {
        let mut return_string: String = "".to_string();
        let mut rx = self.rx.lock().await;
        while let Some(result) = rx.next().await {
            match result {
                Ok(msg) => {
                    if msg.is_close() {

                        println!("{} has disconnected.", username_id);
                        return_string = "Disconnected".to_string();
                        break;
                    } else {
                        // handles client response here----------------
                        if let Ok(str_input) = msg.to_str() {
                            return_string = str_input.to_string();
                        } else {
                            return_string = "Error".to_string();
                        }
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    return_string = "Error".to_string();
                    break;
                }
            }
        }
        return return_string;
    }
}


#[derive(Clone)]
pub struct Lobby {
    pub name: String,
    // Use Arc<Mutex<...>> so the Lobby struct can #[derive(Clone)]
    pub players: Arc<Mutex<Vec<Player>>>,
    pub lobbies: Arc<Mutex<Vec<Lobby>>>,
    pub game_db: SqlitePool,
    deck: Deck,
    pub current_max_bet: i32,
    pub pot: i32,
    pub current_player_count: i32,
    pub max_player_count: i32,
    pub game_state: i32,
    pub first_betting_player: i32,
}

impl Lobby {
    pub async fn new(player_count: Option<i32>, lobby_name: String) -> Self {
        Self {
            name: lobby_name,
            players: Arc::new(Mutex::new(Vec::new())),
            lobbies: Arc::new(Mutex::new(Vec::new())),
            deck: Deck::new(),
            current_max_bet: 0,
            current_player_count: 0,
            max_player_count: player_count.unwrap_or(MAX_PLAYER_COUNT),
            pot: 0,
            game_state: JOINABLE,
            first_betting_player: 0,
            game_db: SqlitePool::connect("sqlite://poker.db").await.unwrap(),
        }
    }

    
    pub fn game_state_machine(){
        // ...
    }

    pub async fn add_player(&mut self, mut player: Player) {
        let mut players = self.players.lock().await;
        player.state = IN_LOBBY;
        self.current_player_count+=1;
        players.push(player);
    }

    pub async fn remove_player(&mut self, server_lobby:Lobby, username: String) {
        let mut players = self.players.lock().await;
        players.retain(|p| p.name != username);
        println!("Player removed from {}: {}",self.name, username);
        self.current_player_count-=1;
        if self.current_player_count == 0{
            server_lobby.remove_lobby(self.name.clone());
        }
    }

    pub async fn add_lobby(&self, lobby: Lobby) {
        let mut lobbies = self.lobbies.lock().await;
        lobbies.push(lobby);
    }

    pub async fn remove_lobby(&self, lobby_name: String) {
        let mut lobbies = self.lobbies.lock().await;
        lobbies.retain(|l| l.name != lobby_name);
    }

    pub async fn get_lobby_names(&self) -> Vec<String> {
        let lobbies = self.lobbies.lock().await;
        lobbies.iter().map(|l| l.name.clone()).collect()
    }

    pub async fn lobby_exists(&self, lobby_name: String) -> bool {
        let lobbies = self.lobbies.lock().await;
        lobbies.iter().any(|l| l.name == lobby_name)
    }

    pub async fn player_join_lobby(&self, username: String, lobby_name: String) -> i32 {
        let mut lobbies = self.lobbies.lock().await;
        println!("Lobby name entered: {}", lobby_name);
        if let Some(lobby) = lobbies.iter_mut().find(|l| l.name == lobby_name) {
            let players = self.players.lock().await;
            if lobby.game_state == JOINABLE {
                println!("Player username: {}", username);
                if let Some(mut player) = players.iter().find(|p| p.name == username.to_string()).cloned() {
                    player.lobby = lobby.clone();
                    lobby.add_player(player).await;
                    return SUCCESS;
                } else {
                    println!("Player not found");
                }
            } else {
                return SERVER_FULL;
            }
        }
        FAILED
    }

    pub async fn get_player_names(&self) -> String {
        let players = self.players.lock().await;
        let message = players.iter().map(|p| p.name.clone()).collect::<Vec<String>>().join(", ");
        message
    }

    pub async fn broadcast(&self, message: String) {
        let players = self.players.lock().await;
        for player in players.iter() {
            let _ = player.tx.send(Message::text(message.clone()));
        }
    }

    pub async fn ready_up(&self, username: String) {
        let mut players = self.players.lock().await;
        if let Some(player) = players.iter_mut().find(|p| p.name == username) {
            player.ready = true;
        }
    }
    
}


fn get_hand_type(hand: &[i32]) -> (i32, i32, i32, i32, i32, i32) {
    assert!(hand.len() == 5);
    
    let mut ranks: Vec<i32> = hand.iter().map(|&card| if card % 13 != 0 { card % 13 } else { 13 }).collect();
    ranks.sort();

    let suits: Vec<i32> = hand.iter().map(|&card| card / 13).collect();

    // Check for flush
    let flush = suits.iter().all(|&suit| suit == suits[0]);

    // Check for straight
    let straight = ranks.windows(2).all(|w| w[1] == w[0] + 1);

    if flush && straight {
        return (8, ranks[4], ranks[4], 0, 0, 0);
    }

    // Check for four of a kind
    for i in 0..2 {
        if ranks[i] == ranks[i + 1] && ranks[i] == ranks[i + 2] && ranks[i] == ranks[i + 3] {
            return if i == 0 {
                (7, ranks[i], ranks[4], 0, 0, 0)
            } else {
                (7, ranks[i], ranks[0], 0, 0, 0)
            };
        }
    }

    // Check for full house
    if ranks[0] == ranks[1] && ranks[3] == ranks[4] {
        if ranks[2] == ranks[0] {
            return (6, ranks[0], ranks[4], 0, 0, 0);
        } else if ranks[2] == ranks[4] {
            return (6, ranks[4], ranks[0], 0, 0, 0);
        }
    }

    if flush {
        return (5, ranks[4], ranks[3], ranks[2], ranks[1], ranks[0]);
    }

    if straight {
        return (4, ranks[4], 0, 0, 0, 0);
    }

    // Check 3 of a kind
    for i in 0..3 {
        if ranks[i] == ranks[i + 1] && ranks[i] == ranks[i + 2] {
            return match i {
                0 => (3, ranks[i], ranks[4], ranks[3], 0, 0),
                1 => (3, ranks[i], ranks[4], ranks[0], 0, 0),
                2 => (3, ranks[i], ranks[1], ranks[0], 0, 0),
                _ => unreachable!(),
            };
        }
    }

    // Check two pair
    if ranks[0] == ranks[1] && ranks[2] == ranks[3] {
        return (3, ranks[0].max(ranks[2]), ranks[0].min(ranks[2]), ranks[4], 0, 0);
    } else if ranks[0] == ranks[1] && ranks[3] == ranks[4] {
        return (3, ranks[0].max(ranks[3]), ranks[0].min(ranks[3]), ranks[2], 0, 0);
    } else if ranks[1] == ranks[2] && ranks[3] == ranks[4] {
        return (3, ranks[1].max(ranks[3]), ranks[1].min(ranks[3]), ranks[0], 0, 0);
    }

    // Check one pair
    for i in 0..4 {
        if ranks[i] == ranks[i + 1] {
            return match i {
                0 => (2, ranks[i], ranks[4], ranks[3], ranks[2], 0),
                1 => (2, ranks[i], ranks[4], ranks[3], ranks[0], 0),
                2 => (2, ranks[i], ranks[4], ranks[1], ranks[0], 0),
                3 => (2, ranks[i], ranks[2], ranks[1], ranks[0], 0),
                _ => unreachable!(),
            };
        }
    }

    // High card
    (1, ranks[4], ranks[3], ranks[2], ranks[1], ranks[0])
}


//implementations

// fn main() {
//     let mut input = String::new();
//     std::io::stdin().read_line(&mut input).expect("Failed to read line");
//     let hand: Vec<i32> = input.trim().split_whitespace().map(|s| s.parse().unwrap()).collect();
//     let hand_type = get_hand_type(&hand);
//     println!("{:?}", hand_type);
// }