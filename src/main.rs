mod database;
mod game;
mod deck;

use warp::Filter;
use std::sync::Arc;
use database::Database;
use warp::ws::{ Message, WebSocket };
use futures_util::{ StreamExt, SinkExt };
use tokio::sync::{ mpsc, Mutex };
use sqlx::SqlitePool;
use std::collections::HashMap;
// use game::{ show_game_variants, handle_game_selection };
use game::game_state_machine;
use deck::{ Card, Deck };

// Define the Lobby struct
// Lobby struct to manage players and game lobbies
pub struct Lobby {
    players: Mutex<HashMap<String, mpsc::UnboundedSender<Message>>>,
    game_db: SqlitePool,

}
// Implement Lobby methods
impl Lobby {
    async fn new() -> Self {
        Lobby {
            players: Mutex::new(HashMap::new()),

            game_db: SqlitePool::connect("sqlite://poker.db").await.unwrap(),
            
            
        }
    }
    // Add player to the lobby
    async fn add_player(&self, username: String, tx: mpsc::UnboundedSender<Message>) {
        let mut players = self.players.lock().await;
        players.insert(username.clone(), tx.clone());
    }

    // Broadcast a message to all players in a same lobby
    async fn broadcast(&self, game: &String, message: String) {
        let players = self.players.lock().await;
        for player in players.keys() {
            if let Some(tx) = players.get(player) {
                let _ = tx.send(Message::text(message.clone()));
            }
        }
    }

}

// Main function to start the server
#[tokio::main]
async fn main() {
    let db_pool = SqlitePool::connect("sqlite://poker.db").await.expect(
        "Failed to connect to database"
    );

    let database = Arc::new(Database::new(db_pool.clone()));
    let lobby = Arc::new(Lobby::new().await);

    let register_route = warp
        ::path("ws")
        .and(warp::ws())
        .and(with_db(database.clone()))
        .and(with_lobby(lobby.clone()))
        .map(|ws: warp::ws::Ws, db, lobby|
            ws.on_upgrade(move |socket| handle_connection(socket, db, lobby))
        );

    warp::serve(register_route).run(([0, 0, 0, 0], 3030)).await;
}

// Helper functions to pass the database and lobby instances to the WebSocket handler
fn with_db(
    db: Arc<Database>
) -> impl Filter<Extract = (Arc<Database>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || db.clone())
}

// Helper functions to pass the lobby instance to the WebSocket handler
fn with_lobby(
    lobby: Arc<Lobby>
) -> impl Filter<Extract = (Arc<Lobby>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || lobby.clone())
}

// Handle the WebSocket connection
async fn handle_connection(ws: WebSocket, db: Arc<Database>, lobby: Arc<Lobby>) {
    let (mut ws_tx, mut ws_rx) = ws.split();
    let (tx, mut rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let _ = ws_tx.send(msg).await;
        }
    });
    // Send the main menu to the client
    let menu_msg = Message::text(
        "Welcome to Poker! Choose an option:\n1. Login\n2. Register\n3. Exit"
    );
    // Send the menu message to the client
    tx.send(menu_msg).unwrap();
    // Handle the client's choice
    loop {
        if let Some(Ok(msg)) = ws_rx.next().await {
            if let Ok(choice) = msg.to_str() {
                match choice.trim() {
                    "1" => {
                        let prompt_msg = Message::text("Enter your username:");
                        tx.send(prompt_msg).unwrap();

                        if let Some(Ok(username_msg)) = ws_rx.next().await {
                            if let Ok(username) = username_msg.to_str() {
                                let username = username.trim().to_string();
                                match db.login_player(&username).await {
                                    Ok(Some(_id)) => {
                                        tx.send(
                                            Message::text(format!("Welcome back, {}!", username))
                                        ).unwrap();
                                        lobby.add_player(username.clone(), tx.clone()).await;
                                        //broadcast to all players
                                        lobby.broadcast(&username, format!("{} has joined the lobby", username)).await;
                                        
                                        break;
                                    }
                                    _ => {
                                        tx.send(
                                            Message::text("Username not found. Try again.")
                                        ).unwrap();
                                    }
                                }
                            }
                        }
                    }
                    "2" => {
                        let prompt_msg = Message::text("Enter a new username to register:");
                        tx.send(prompt_msg).unwrap();

                        if let Some(Ok(username_msg)) = ws_rx.next().await {
                            if let Ok(username) = username_msg.to_str() {
                                let username = username.trim().to_string();
                                match db.register_player(&username).await {
                                    Ok(_) => {
                                        let success_msg = Message::text(
                                            format!("Registration successful! Welcome, {}! You are now in the lobby.", username)
                                        );
                                        tx.send(success_msg).unwrap();
                                        // Add the player to the main lobby
                                        lobby.add_player(username.clone(), tx.clone()).await;
                                        lobby.broadcast(&username, format!("{} has joined the lobby", username)).await;
                                        break;
                                    }
                                    Err(_) => {
                                        let error_msg = Message::text(
                                            "Registration failed. Try again."
                                        );
                                        tx.send(error_msg).unwrap();
                                    }
                                }
                            }
                        }
                    }
                    "3" => {
                        tx.send(Message::text("Goodbye!")).unwrap();
                        return;
                    }
                    _ => {
                        tx.send(Message::text("Invalid option.")).unwrap();
                    }
                }
            }
        }
    }
    game_state_machine(&lobby).await;
    // show_game_variants(&tx).await;
    // if let Some(Ok(game_choice_msg)) = ws_rx.next().await {
    //     if let Ok(choice) = game_choice_msg.to_str() {
    //         handle_game_selection(choice, &tx).await;
    //     }
    // }
}