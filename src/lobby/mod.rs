//!
use super::*;
use crate::Deck;
use futures_util::future::ready;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, mpsc::UnboundedSender};
use warp::{filters::ws::WebSocket, ws::Message};
// use warp::filters::ws::SplitStream;


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
pub const GAME_LOBBY_EMPTY: i32 = 103;
pub const GAME_LOBBY_NOT_EMPTY: i32 = 104;
pub const GAME_LOBBY_FULL: i32 = 105;


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
    pub lobby: Arc<Mutex<Lobby>>,
}

impl Player {
    pub async fn remove_player(&self, server_lobby: Arc<Mutex<Lobby>>, db: Arc<Database>) {
        db.update_player_stats(&self).await.unwrap();

        server_lobby.lock().await
            .remove_player(self.name.clone())
            .await;
        server_lobby.lock().await
            .broadcast(format!("{} has left the server.", self.name))
            .await;
    }

    pub async fn get_player_input(&mut self) -> String {
        let mut return_string: String = "".to_string();
        let mut rx = self.rx.lock().await;
        while let Some(result) = rx.next().await {
            match result {
                Ok(msg) => {
                    if msg.is_close() {
                        // handle all cases of disconnection here during game---------------------
                        println!("{} has disconnected.", self.name);
                        match self.state {
                            IN_GAME => {
                                self.state = IN_LOBBY;
                            },
                            ANTE => {
                                self.state = FOLDED;
                            },
                            DEAL_CARDS => {
                                self.state = FOLDED;
                            },
                            FIRST_BETTING_ROUND => {
                                self.state = FOLDED;

                            },
                            DRAW => {
                                self.state = FOLDED;

                            },
                            SECOND_BETTING_ROUND => {
                                self.state = FOLDED;
                            },
                            _ => {
                                self.state = IN_LOBBY;
                            }
                        }
                        println!("Disconnect-------------");
                        return "Disconnect".to_string(); // pass flag back
                    } else {
                        // handles client response here----------------
                        if let Ok(str_input) = msg.to_str() {
                            return_string = str_input.to_string();
                        } else {
                            return_string = "Error".to_string();
                        }
                        return return_string;
                    }
                },
                Err(e) => {
                    eprintln!("Error: {}", e);
                    return_string = "Error".to_string();
                    return return_string;
                }
            }
        }
        return return_string;
    }

    pub async fn player_join_lobby(&mut self, server_lobby: Arc<Mutex<Lobby>>, lobby_name: String) -> i32 {
        let lobbies = server_lobby.lock().await.lobbies.lock().await.clone();
        println!("Lobby name entered: {}", lobby_name);
        for lobby in lobbies {
            let mut lobby_guard = lobby.lock().await;
            if lobby_guard.name == lobby_name {
                if lobby_guard.game_state == JOINABLE {
                    lobby_guard.add_player(self.clone()).await;
                    self.lobby = lobby.clone();
                    return SUCCESS;
                } else {
                    return SERVER_FULL;
                }
            }
        }
        FAILED
    }
}



#[derive(Clone)]
pub struct Lobby {
    pub name: String,
    // Use Arc<Mutex<...>> so the Lobby struct can #[derive(Clone)]
    pub players: Arc<Mutex<Vec<Player>>>,
    pub lobbies: Arc<Mutex<Vec<Arc<Mutex<Lobby>>>>>,
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

    pub async fn listen_for_messages(&self, mut game_rx: mpsc::UnboundedReceiver<String>) -> String {
        while let Some(message) = game_rx.recv().await {
            println!("Received message: {}", message);
            return message;
        }
        "".to_string()
    }

    pub async fn increment_player_count(&mut self) {
        self.current_player_count += 1;
    }

    pub async fn decrement_player_count(&mut self) {
        self.current_player_count -= 1;
    }

    pub async fn get_player_count(&self) -> i32 {
        self.current_player_count
    }

    pub async fn add_player(&mut self, mut player: Player) {
        let mut players = self.players.lock().await;
        player.state = IN_LOBBY;
        players.push(player);
        self.current_player_count += 1;
        if self.current_player_count == self.max_player_count {
            self.game_state = GAME_LOBBY_FULL;
        } else {
            self.game_state = JOINABLE;
        }
    }

    pub async fn remove_player(&mut self, username: String) -> i32{
        let mut players = self.players.lock().await;
        players.retain(|p| p.name != username);
        println!("Player removed from {}: {}",self.name, username);
        self.current_player_count -= 1;
        if self.current_player_count == 0{
            return GAME_LOBBY_EMPTY;
        } else {
            self.game_state = JOINABLE;
        }
        GAME_LOBBY_NOT_EMPTY
    }

    pub async fn add_lobby(&self, lobby: Arc<Mutex<Lobby>>) {
        let mut lobbies = self.lobbies.lock().await;
        lobbies.push(lobby);
    }

    pub async fn remove_lobby(&self, lobby_name: String) {
        let mut lobbies = self.lobbies.lock().await;
        let mut i = 0;
        while i < lobbies.len() {
            if lobbies[i].lock().await.name == lobby_name {
                lobbies.remove(i);
            } else {
                i += 1;
            }
        }
    }

    pub async fn get_lobby_names_and_status(&self) -> Vec<(String, i32)> {
        let lobbies = self.lobbies.lock().await;
        let mut names_and_status = Vec::new();
        for lobby in lobbies.iter() {
            let lobby_guard = lobby.lock().await;
            names_and_status.push((lobby_guard.name.clone(), lobby_guard.game_state));
        }
        names_and_status
    }

    pub async fn lobby_exists(&self, lobby_name: String) -> bool {
        let lobbies = self.lobbies.lock().await;
        for lobby in lobbies.iter() {
            if lobby.lock().await.name == lobby_name {
                return true;
            }
        }
        false
    }

    pub async fn get_player_names(&self) -> String {
        let players = self.players.lock().await;
        let message = players
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<String>>()
            .join("\n");
        message
    }

    // pub async fn broadcast(&self, message: String) {
    //     let players = self.players.lock().await;
    //     for player in players.iter() {
    //         let _ = player.tx.send(Message::text(message.clone()));
    //     }
    // }

    pub async fn broadcast(&self, message: String) {
        println!("Broadcasting: {}", message);
        let players = self.players.lock().await;
        let mut tasks = Vec::new();
        for player in players.iter() {
            let tx = player.tx.clone();
            let msg = Message::text(message.clone());
            tasks.push(tokio::spawn(async move {
                let _ = tx.send(msg);
            }));
        }
        // Wait for all tasks to complete
        for task in tasks {
            let _ = task.await;
        }
    }

    pub async fn lobby_wide_send(&self, players_tx: Vec<UnboundedSender<Message>>, message: String) {
        let mut tasks = Vec::new();
        for tx in players_tx.iter().cloned() {
            let msg = Message::text(message.clone());
            tasks.push(tokio::spawn(async move {
                let _ = tx.send(msg);
            }));
        }
        // Wait for all tasks to complete
        for task in tasks {
            let _ = task.await;
        }
    }

    pub async fn ready_up(&self, username: String) -> (i32,i32) {
        let mut players = self.players.lock().await;
        // self.broadcast(format!("{} is ready!", username)).await;
        if let Some(player) = players.iter_mut().find(|p| p.name == username) {
            player.ready = true;
        }
        let mut ready_player_count = 0;
        for player in players.iter() {
            if player.ready {
                ready_player_count += 1;
            }
        }
        let players_tx = players.iter().map(|p| p.tx.clone()).collect::<Vec<_>>();
        self.lobby_wide_send(players_tx, format!("{} is ready!", username)).await;
        return (ready_player_count, self.current_player_count);
    }

    async fn deal_cards(&mut self) {
        let mut players = self.players.lock().await;
        for _ in 0..5 {
            for player in players.iter_mut() {
                player.hand.push(self.deck.deal());
            }
        }
        // print the hands to the players
        let players_tx = players.iter()
            .filter(|p| p.state != FOLDED)
            .map(|p| p.tx.clone())
            .collect::<Vec<_>>(); // get all tx's
        let players_hands = players.iter()
            .filter(|p| p.state != FOLDED)
            .map(|p| p.hand.clone())
            .collect::<Vec<_>>(); // get all hands
        self.display_hand(players_tx.clone(), players_hands.clone()).await;
    }

    async fn betting_round(&mut self, round: i32) {

        let mut players = self.players.lock().await;
        let players_tx = players.iter().map(|p| p.tx.clone()).collect::<Vec<_>>();
        println!("Current round: {}", round);
        // ensure all players have current_bet set to 0

        let mut current_player_index = self.first_betting_player;
        let mut current_lobby_bet = 0; // resets to 0 every betting round
        let mut players_remaining = self.current_player_count;
        let mut check_flag = false;
        if round == FIRST_BETTING_ROUND {
            current_lobby_bet = 10; // first betting round starts with the ante
            check_flag = true; // players can check if its first betting round with ante
        }
        else {
            for player in players.iter_mut() {
                player.current_bet = 0;
            }
        }
        while players_remaining > 0 {
            let player = &mut players[current_player_index as usize];
            if player.state == FOLDED || player.state == ALL_IN {
                current_player_index = (current_player_index + 1) % self.current_player_count;
                players_remaining -= 1;
                continue;
            }
            if round == ANTE {
                if player.wallet > 10 {
                    println!("Reached");
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
                    "Choose an option:\n1. Check\n2. Raise\n3. Call\n4. Fold\n5. All-in\n\nCurrent bet: {}\nCurrent Pot: {}",
                    player.current_bet, self.pot
                );
                let _ = player.tx.send(Message::text(message));
                loop {
                    let choice = player.get_player_input().await;
                    println!("player input: {}", choice.clone());

                    //

                        
                    //

                    match choice.as_str() {
                        "1" => {
                            if current_lobby_bet == 0 || check_flag == true {
                                player.state = CHECKED;
                                println!("checked");
                                // self.broadcast(format!("{} has checked.", player.name)).await;
                                self.lobby_wide_send(players_tx.clone(), format!("{} has checked.", player.name)).await;
                                players_remaining -= 1; // on valid moves, decrement the amount of players to make a move
                                break;
                            } else {
                                player.tx.send(Message::text("Invalid move: You can't check, there's a bet to call.",)).ok();
                            }
                        }
                        "2" => {
                            if player.state == RAISED {
                                player.tx.send(Message::text("You've already raised this round.\nCall or fold.")).ok();
                                continue;
                            }
                            let bet_diff = current_lobby_bet - player.current_bet;
                            if current_lobby_bet > 0 {
                                if player.wallet <= (current_lobby_bet - player.current_bet) {
                                    player.tx.send(Message::text("Invalid move: not enough cash to raise.\nCall or fold.")).ok();
                                    continue;
                                }
                                // print the minimum the player has to bet to stay in the game
                                let _ = player.tx.send(Message::text(format!("Current minimum bet is: {}", bet_diff)));
                            }
                            else {
                                let _ = player.tx.send(Message::text("Bet must be greater than 0."));
                            }
                            let _ = player.tx.send(Message::text(format!("Your current bet is: {}\nYour wallet balance: {}", player.current_bet, player.wallet)));
                            let _ = player.tx.send(Message::text("Enter your bet amount:"));
                            loop {
                                let bet_amount = player.get_player_input().await;
                                if let Ok(bet) = bet_amount.parse::<i32>() {
                                    // doesnt allow calling (all in case included) or raising if the player doesnt have enough money
                                    if bet > player.wallet || bet <= bet_diff || bet <= 0 {
                                        player.tx.send(Message::text("Invalid raise.")).ok();
                                    }
                                    else {
                                        if bet == player.wallet {
                                            player.state = ALL_IN;
                                            // self.broadcast(format!("{} has gone all in!", player.name)).await;
                                            self.lobby_wide_send(players_tx.clone(), format!("{} has gone all in!", player.name)).await;
                                        }
                                        else{
                                            player.state = RAISED;
                                        }
                                        player.wallet -= bet;
                                        player.current_bet += bet;
                                        self.pot += bet;
                                        current_lobby_bet = player.current_bet;

                                        // self.broadcast(format!("{} has raised the pot to: {}", player.name, player.current_bet)).await;
                                        self.lobby_wide_send(players_tx.clone(), format!("{} has raised the pot to: {}", player.name, self.pot)).await;
                                        // reset the betting cycle so every player calls/raises the new max bet or folds
                                        players_remaining = self.current_player_count - 1;
                                        check_flag = false;
                                        break;
                                    }
                                } else {
                                    player.tx.send(Message::text("Invalid raise : not a number.")).ok();
                                }
                            }
                            break;
                        }
                        "3" => {
                            let call_amount = current_lobby_bet - player.current_bet;
                            if call_amount > player.wallet {
                                player.tx.send(Message::text("Invalid move: not enough cash.\nAll in or fold!")).ok();
                            } else if call_amount == 0 {
                                player.tx.send(Message::text("Invalid move: no bet to call.")).ok();
                            } else {
                                player.wallet -= call_amount;
                                player.current_bet += call_amount;
                                self.pot += call_amount;
                                player.state = CALLED;
                                // self.broadcast(format!("{} has called the bet.", player.name)).await;
                                self.lobby_wide_send(players_tx.clone(), format!("{} has called the bet.", player.name)).await;
                                players_remaining -= 1;
                                break;
                            }
                        }
                        "4" => {
                            player.state = FOLDED;
                            // self.broadcast(format!("{} has folded.", player.name)).await;
                            self.lobby_wide_send(players_tx.clone(), format!("{} has folded.", player.name)).await;
                            players_remaining -= 1;
                            break;
                        }
                        "5" => {
                            // all in
                            // side pots not considered yet
                            if player.state == RAISED {
                                player.tx.send(Message::text("You've already raised this round.\nCall or fold.")).ok();
                                continue;
                            }
                            if player.wallet > 0 {
                                self.pot += player.wallet;
                                player.current_bet += player.wallet;
                                player.wallet -= player.wallet;
                                if player.current_bet > current_lobby_bet {
                                    current_lobby_bet = player.current_bet;
                                }
                                player.state = ALL_IN;
                                // self.broadcast(format!("{} has gone all in!", player.name)).await;
                                self.lobby_wide_send(players_tx.clone(), format!("{} has gone all in!", player.name)).await;
                                players_remaining = self.current_player_count - 1; // reset the betting cycle
                                break;
                            }
                            check_flag = false;
                        },
                        "Disconnected" => {
                            // self.broadcast(format!("{} has disconnected and folded.", player.name)).await;
                            self.lobby_wide_send(players_tx.clone(), format!("{} has disconnected and folded.", player.name)).await;
                            break;
                        }
                        _ => {
                            player.tx.send(Message::text("Invalid action, try again.")).ok();
                        }
                    }
                }
            }

            // Move to next player
            current_player_index = (current_player_index + 1) % self.current_player_count;
            // players_remaining -= 1; // ensure we give everyone a change to do an action
        }
        println!("End of betting round: {}", round);
    }

    async fn drawing_round(&mut self){
        //As the drawing round starts, we will check if their status is folded, if it is, we will skip them
        //else we will continue the drawing round for that player and we will display a input menu of "Stand Pat or Exchange cards"
        //if the player chooses to exchange cards, we will remove the cards from their hand and deal them new cards the logic for this will be
        //once players chooses the index of cards they want to change, we will remove those cards from their hand and deal them new cards
        //If they stand pat nothing will happen and it will move to the next player
        //Once the cards are swap we will quickly display the cards to the player only.
        //Once all players have swapped their cards, we will move to the next betting round
        let mut players = self.players.lock().await;
    
        let players_tx = players.iter()
            .filter(|p| p.state != FOLDED)
            .map(|p| p.tx.clone())
            .collect::<Vec<_>>(); // get all tx's
        let players_hands = players.iter()
            .filter(|p| p.state != FOLDED)
            .map(|p| p.hand.clone())
            .collect::<Vec<_>>(); // get all hands

        self.display_hand(players_tx.clone(), players_hands.clone()).await;
        for player in players.iter_mut(){
            if player.state == FOLDED || player.state == ALL_IN {continue};
            println!("Drawing round for: {}", player.name);

            player.tx.send(Message::text("Drawing round!")).ok();
            let message = format!(
                //displays the user hand
                "Choose an option:\n    stand_pat\n    exchange [indicies you wanna exchange(comma seperated)]"
            );
            let _ = player.tx.send(Message::text(message));
           // Get player input
            loop {
                let input = player.get_player_input().await;

                if input == "stand_pat" {
                    break;
                    //The input looks like "exchange 1,2,3" where 1,2,3 are the indices of the cards the player wants to exchange
                } else if input.starts_with("exchange ") {
                    // Extracting indices from the input
                    if let Some(indices_str) = input.strip_prefix("exchange ") {
                        if !indices_str.chars().all(|c| c.is_digit(10) || c == ',' || c.is_whitespace()) {
                            let _ = player.tx.send(Message::text("Invalid format. Use numbers separated by commas (e.g., 'exchange 1,2,3')."));
                            continue;
                        }
                        let indices: Vec<usize> = indices_str
                        .split(',')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty()) // Ensure no empty values
                        .filter_map(|s| s.parse().ok()) // Convert to numbers
                        .collect();

                        let mut valid_indices: Vec<usize> = indices
                            .iter()
                            .cloned()
                            .filter(|&i| i > 0 && i <= player.hand.len()) // Ensure within bounds
                            .map(|i| i - 1) // Convert to zero-based index
                            .collect();

                        valid_indices.sort();
                        valid_indices.dedup(); // Ensure uniqueness

                        if valid_indices.len() == indices.len() && !valid_indices.is_empty() {
                            let mut new_hand = Vec::new();
                            for (i, card) in player.hand.iter().enumerate() {
                                if !valid_indices.contains(&i) {
                                    new_hand.push(*card);
                                }
                            }
                            for _ in &valid_indices {
                                new_hand.push(self.deck.deal());
                            }
                            player.hand = new_hand;
                            self.lobby_wide_send(players_tx.clone(),format!("{} has exchanged {} cards.", player.name, valid_indices.len())).await;
                            break;
                        } 
                        else {
                            let _ = player.tx.send(Message::text("Invalid indices. Ensure they are within range and correctly formatted."));
                        }
                    }
                }
                else if input.starts_with("Disconnected") {
                    // self.broadcast(format!("{} has disconnected and folded.", player.name)).await;
                    self.lobby_wide_send(players_tx.clone(), format!("{} has disconnected.", player.name)).await;
                    break;
                }
                else {
                    let _ = player.tx.send(Message::text("Invalid action. Please try again."));
                }
            }
        }
    }

    async fn showdown(&self) {
        let players: Vec<Player> = self.players.lock().await.to_vec();
        let players_tx = players.iter().map(|p| p.tx.clone()).collect::<Vec<_>>();
        let mut winning_players: Vec<Player> = Vec::new(); // keeps track of winning players at the end, accounting for draws
        let mut winning_players_names: Vec<String> = Vec::new();
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
                winning_players_names.clear();
                winning_players.push(player.clone());
                winning_players_names.push(player.name.clone());
            } else if player_hand_type.0 == winning_hand.0 && player_hand_type.1 == winning_hand.1 {
                winning_players.push(player.clone());
                winning_players_names.push(player.name.clone());
            }
        }
        let winning_player_count = winning_players.len();
        let pot_share = self.pot / winning_player_count as i32;
        for player in winning_players {
            let mut player = player.clone();
            player.wallet += pot_share;
        }
        let winner_names = winning_players_names.join(", ");
        self.lobby_wide_send(players_tx, format!("Winner: {}", winner_names)).await;
    }

    async fn change_player_state(&self, state: i32) {
        // loop through players and change their state
        let mut players = self.players.lock().await;
        for player in players.iter_mut() {
            println!("Changing {} state to: {}", player.name, state);
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

    async fn display_hand(&self, players_tx: Vec<UnboundedSender<Message>>, players_hands: Vec<Vec<i32>>) {
        // let players = self.players;
        let mut message: String;
        let mut index = 0;
        for tx in players_tx.iter().cloned(){
            let mut translated_cards: String = Default::default();
            for card in players_hands[index].iter().cloned() {
                translated_cards.push_str(&self.translate_card(card.clone()).await);
                translated_cards.push_str(", ");
            }
            message = format!("Your hand: {}", translated_cards.trim_end_matches(", "));
            let _ = tx.send(Message::text(message.clone()));
            index += 1;
        }
    }

    // 
    // async fn update_db(&self) {
    //     // update the database with the new player stats
    //     let players = self.players.lock().await;
    //     for player in players.iter() {
    //         self.game_db.update_player_stats(&player).await.unwrap();
    //     }
    // }

    // async fn get_tx_vec(&self) {
        
    // }

    pub async fn start_game(&mut self){
        // change lobby state first so nobody can try to join anymore
        println!("Game started!");

        // await all players to ready up


        //

        


        self.game_state = START_OF_ROUND;
        self.change_player_state(IN_GAME).await;
        
        self.game_state_machine().await;
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
                    // self.display_hand().await;
                    self.game_state = FIRST_BETTING_ROUND;
                }
                FIRST_BETTING_ROUND => {
                    self.broadcast("First betting round!".to_string()).await;
                    self.betting_round(FIRST_BETTING_ROUND).await;
                    self.game_state = DRAW;
                    self.broadcast(format!("First betting round complete!\nCurrent pot: {}", self.pot)).await;
                }
                DRAW => {
                    self.broadcast("Drawing round!".to_string()).await;
                    self.drawing_round().await;
                    // self.display_hand().await;
                    self.game_state = SECOND_BETTING_ROUND;
                }
                SECOND_BETTING_ROUND => {
                    self.broadcast("Second betting round!".to_string()).await;
                    self.betting_round(FIRST_BETTING_ROUND).await;
                    self.broadcast(format!("Second betting round complete!\nCurrent pot: {}", self.pot)).await;
                    self.game_state = SHOWDOWN;
                }
                SHOWDOWN => {
                    self.game_state = END_OF_ROUND;
                }
                END_OF_ROUND => {
                    self.game_state = UPDATE_DB;
                }
                UPDATE_DB => {
                    self.game_state = IN_LOBBY;
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
