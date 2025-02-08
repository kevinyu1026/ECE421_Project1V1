//!
use super::*;
use crate::Deck;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use warp::{filters::ws::WebSocket, ws::Message};

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
// const ANTE_ROUND: i32 = 0;

// Player state definitions
pub const READY: i32 = 0;
const FOLDED: i32 = 1;
const ALL_IN: i32 = 2;
const CHECKED: i32 = 3;
const CALLED: i32 = 4;
const RAISED: i32 = 8;
pub const IN_LOBBY: i32 = 5;
pub const IN_SERVER: i32 = 6;
const IN_GAME: i32 = 7;

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

        server_lobby
            .remove_player(server_lobby.clone(), self.name.clone())
            .await;
        server_lobby
            .broadcast(format!("{} has left the server.", self.name))
            .await;
    }

    pub async fn get_player_input(&self) -> String {
        let mut return_string: String = "".to_string();
        let mut rx = self.rx.lock().await;
        while let Some(result) = rx.next().await {
            match result {
                Ok(msg) => {
                    if msg.is_close() {
                        println!("{} has disconnected.", self.name);
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
            current_player_count: 0,
            max_player_count: player_count.unwrap_or(MAX_PLAYER_COUNT),
            pot: 0,
            game_state: JOINABLE,
            first_betting_player: 0,
            game_db: SqlitePool::connect("sqlite://poker.db").await.unwrap(),
        }
    }

    pub async fn add_player(&mut self, mut player: Player) {
        let mut players = self.players.lock().await;
        player.state = IN_LOBBY;
        self.current_player_count += 1;
        players.push(player);
    }

    pub async fn remove_player(&mut self, server_lobby: Lobby, username: String) {
        let mut players = self.players.lock().await;
        players.retain(|p| p.name != username);
        println!("Player removed from {}: {}", self.name, username);
        self.current_player_count -= 1;
        if self.current_player_count == 0 {
            server_lobby.remove_lobby(self.name.clone()).await;
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
                if let Some(mut player) = players
                    .iter()
                    .find(|p| p.name == username.to_string())
                    .cloned()
                {
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
        let message = players
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<String>>()
            .join(", ");
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

    async fn deal_cards(&mut self) {
        let mut players = self.players.lock().await;
        for _ in 0..5 {
            for player in players.iter_mut() {
                player.hand.push(self.deck.deal());
            }
        }
    }

    async fn betting_round(&mut self, round: i32) {
        let mut players = self.players.lock().await;
        // ensure all players have current_bet set to 0

        for player in players.iter_mut() {
            player.current_bet = 0;
        }
        let mut current_player_index = self.first_betting_player;
        let mut current_lobby_bet = 0; // resets to 0 every betting round
        let mut players_remaining = self.current_player_count;

        while players_remaining > 1 {
            let player = &mut players[current_player_index as usize];
            if player.state == FOLDED || player.state == ALL_IN{
                current_player_index = (current_player_index + 1) % self.current_player_count;
                continue;
            }
            if round == ANTE {
                if player.wallet > 10 {
                    self.pot += 10;
                    player.wallet -= 10;
                    player.current_bet += 10;
                    players_remaining -= 1;
                }
                else {
                    player.state = FOLDED; // they couldnt afford ANTE, gettt emmm outttt
                }
            } else {
                let message = format!(
                    "Choose an option:\n1. Check\n2. Bet\n3. Call\n4. Fold\n5. All-in\n\nCurrent bet: {}\nCurrent Pot: {}",
                    player.current_bet, self.pot
                );
                let _ = player.tx.send(Message::text(message));
                loop {
                    let choice = player.get_player_input().await;
                    match choice.as_str() {
                        "1" => {
                            if current_lobby_bet == 0 {
                                player.state = CHECKED;
                                players_remaining -= 1; // on valid moves, decrement the amount of players to make a move
                            } else {
                                player
                                    .tx
                                    .send(Message::text(
                                        "Invalid move: You can't check, there's a bet to call.",
                                    ))
                                    .ok();
                            }
                        }
                        "2" => {
                            let bet_diff = current_lobby_bet - player.current_bet;
                            if current_lobby_bet > 0 {
                                // print the minimum the player has to bet to stay in the game
                                let _ = player.tx.send(Message::text(format!("Current minimum bet is: {}", bet_diff)));
                            }
                            else {
                                let _ = player.tx.send(Message::text("Bet must be greater than 0."));
                                let _ = player.tx.send(Message::text("Enter your bet amount:"));
                            }
                            loop {
                                let bet_amount = player.get_player_input().await;
                                if let Ok(bet) = bet_amount.parse::<i32>() {
                                    if bet > player.wallet || bet < bet_diff {
                                        player.tx.send(Message::text("Invalid bet.")).ok();
                                    }
                                    else {
                                        if bet == player.wallet {
                                            // if deciding to all in, broadcast it?
                                            player.state = ALL_IN;
                                        }
                                        player.wallet -= bet;
                                        player.current_bet += bet;
                                        self.pot += bet;
                                        current_lobby_bet = player.current_bet;
                                        
                                        if bet == bet_diff {
                                            player.state = CALLED;
                                            players_remaining -= 1; // player didnt raise, they matched the bet
                                        }
                                        else{
                                            player.state = RAISED;
                                            // reset the betting cycle so every player calls/raises the new max bet or folds
                                            players_remaining = self.current_player_count;
                                        }
                                        break;
                                    }
                                } else {
                                    player
                                        .tx.send(Message::text("Invalid bet: not a number.")).ok();
                                }
                            }
                        }
                        "3" => {
                            let call_amount = current_lobby_bet - player.current_bet;
                            if call_amount > player.wallet {
                                player.tx.send(Message::text("Invalid move: not enough cash.")).ok();
                            } else {
                                player.wallet -= call_amount;
                                player.current_bet += call_amount;
                                self.pot += call_amount;
                                player.state = CALLED;
                                players_remaining -= 1;
                            }
                        }
                        "4" => {
                            player.state = FOLDED;
                            players_remaining -= 1;
                        }
                        "5" => {
                            // all in
                            if player.wallet > 0 {
                                self.pot += player.wallet;
                                player.wallet -= player.wallet;
                                player.current_bet += player.wallet;
                                if player.current_bet > current_lobby_bet {
                                    current_lobby_bet = player.current_bet;
                                }
                                player.state = ALL_IN;
                                players_remaining -= 1;
                            }
                        }
                        _ => {
                            player.tx.send(Message::text("Invalid action.")).ok();
                        }
                    }
                }
            }

            // Move to next player
            current_player_index = (current_player_index + 1) % self.current_player_count;
            // players_remaining -= 1; // ensure we give everyone a change to do an action
        }
    }

    async fn showdown(&self) {
        let players: Vec<Player> = self.players.lock().await.to_vec();
        let mut winning_players: Vec<Player> = Vec::new(); // keeps track of winning players at the end, accounting for draws
        let mut winning_hand = (0, 0, 0, 0, 0, 0); // keeps track of current highest hand, could change when incrementing between players
        for player in players {
            if player.state == FOLDED {
                continue;
            };
            let player_hand = player.hand.clone();
            let player_hand_type = get_hand_type(&player_hand);
            if player_hand_type.0 > winning_hand.0
                || (player_hand_type.0 == winning_hand.0 && player_hand_type.1 > winning_hand.1)
            {
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

    async fn change_player_state(&self, state: i32) {
        // loop through players and change their state
        let mut players = self.players.lock().await;
        for player in players.iter_mut() {
            player.state = state;
        }
    }

    async fn translate_card(&self, card: i32) -> String {
        let mut cardStr: String = Default::default();
        let rank: i32 = card % 13;

        if rank == 0 {
            cardStr.push_str("Ace");
        } else if rank <= 9 {
            cardStr.push_str(&(rank + 1).to_string());
        } else if rank == 10 {
            cardStr.push_str("Jack");
        } else if rank == 11 {
            cardStr.push_str("Queen");
        } else if rank == 12 {
            cardStr.push_str("King");
        }

        let suit: i32 = card / 13;
        if suit == 0 {
            cardStr.push_str(" Hearts");
        } else if suit == 1 {
            cardStr.push_str(" Diamond");
        } else if suit == 2 {
            cardStr.push_str(" Spade");
        } else if suit == 3 {
            cardStr.push_str(" Club");
        }
        return cardStr;
    }

    async fn display_hand(&self) {
        let mut players = self.players.lock().await;
        let mut message: String;

        for player in players.iter_mut() {
            let mut translated_cards: String = Default::default();
            for card in &player.hand {
                translated_cards.push_str(&self.translate_card(*card).await);
                translated_cards.push_str(", ");
            }
            message = format!("Your hand: {}", translated_cards.trim_end_matches(", "));
            let _ = player.tx.send(Message::text(message.clone()));
        }
    }

    pub async fn start_game(&mut self) {
        // change lobby state first so nobody can try to join anymore
        self.game_state = START_OF_ROUND;
        self.change_player_state(IN_GAME);
        self.game_state_machine();
    }

    async fn game_state_machine(&mut self) {
        loop {
            match self.game_state {
                START_OF_ROUND => {
                    self.game_state = ANTE;
                }
                ANTE => {
                    self.broadcast("Ante round!\nEveryone adds $10 to the pot.".to_string()).await;
                    self.betting_round(ANTE).await;
                    self.broadcast(format!("Current pot: {}", self.pot)).await;
                    self.game_state = DEAL_CARDS;
                }
                DEAL_CARDS => {
                    self.broadcast("Dealing cards...".to_string()).await;
                    self.deal_cards().await;
                    // display each players hands to them
                    self.display_hand().await;
                    self.game_state = FIRST_BETTING_ROUND;
                }
                FIRST_BETTING_ROUND => {
                    self.broadcast("First betting round!".to_string()).await;
                    self.betting_round(FIRST_BETTING_ROUND).await;
                    self.game_state = DRAW;
                }
                DRAW => {
                    self.game_state = SECOND_BETTING_ROUND;
                }
                SECOND_BETTING_ROUND => {
                    self.game_state = SHOWDOWN;
                }
                SHOWDOWN => {
                    self.game_state = END_OF_ROUND;
                }
                END_OF_ROUND => {
                    self.game_state = UPDATE_DB;
                }
                UPDATE_DB => {
                    self.game_state = JOINABLE;
                }
                _ => {
                    panic!("Invalid game state: {}", self.game_state);
                }
            }
        }
    }
}

fn get_hand_type(hand: &[i32]) -> (i32, i32, i32, i32, i32, i32) {
    assert!(hand.len() == 5);

    let mut ranks: Vec<i32> = hand
        .iter()
        .map(|&card| if card % 13 != 0 { card % 13 } else { 13 })
        .collect();
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
        return (
            3,
            ranks[0].max(ranks[2]),
            ranks[0].min(ranks[2]),
            ranks[4],
            0,
            0,
        );
    } else if ranks[0] == ranks[1] && ranks[3] == ranks[4] {
        return (
            3,
            ranks[0].max(ranks[3]),
            ranks[0].min(ranks[3]),
            ranks[2],
            0,
            0,
        );
    } else if ranks[1] == ranks[2] && ranks[3] == ranks[4] {
        return (
            3,
            ranks[1].max(ranks[3]),
            ranks[1].min(ranks[3]),
            ranks[0],
            0,
            0,
        );
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
