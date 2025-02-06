use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use sqlx::SqlitePool;
use crate::Card;
use warp::ws::Message;


// Define Player struct
pub struct Player {
    pub name: String,
    pub id: String,
    pub hand: Vec<Card>,
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
    // game_state: String,
}

impl Lobby {
    pub async fn new() -> Self {
        Lobby {
            players: Mutex::new(Vec::new()),
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

    async fn deal_cards(&self) {
        let mut players = self.players.lock().await;
        let mut deck = Card::generate_deck();
        for player in players.iter_mut() {
            player.hand.push(deck.pop().unwrap());
            player.hand.push(deck.pop().unwrap());
        }
    }
}