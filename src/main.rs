mod database;
mod game;
mod deck;
mod lobby;

use warp::Filter;
use std::sync::Arc;
use database::Database;
use warp::ws::{ Message, WebSocket };
use futures_util::{ StreamExt, SinkExt, FutureExt };
use tokio::sync::{ mpsc, Mutex };
use sqlx::SqlitePool;
use std::collections::HashMap;
use uuid::Uuid;
use game::game_state_machine;
use deck::{ Card, Deck };
use tokio::time::{ sleep, Duration };
use lobby::*;
// use lobby::Player;
use tokio_tungstenite::connect_async;



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

    warp::serve(register_route).run(([0, 0, 0, 0], 1112)).await;
}

// Helper functions to pass the database and lobby instances to the WebSocket handler
fn with_db(
    db: Arc<Database>
) -> impl Filter<Extract = (Arc<Database>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || db.clone())
}

fn with_lobby(
    lobby: Arc<Lobby>
) -> impl Filter<Extract = (Arc<Lobby>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || lobby.clone())
}

// Handle the WebSocket connection
async fn handle_connection(ws: WebSocket, db: Arc<Database>, lobby: Arc<Lobby>) {
    let (mut ws_tx, mut ws_rx) = ws.split();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut username_id = "".to_string(); // saving so we can match the player to the player object
    // Spawn a task to send messages to the client
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let _ = ws_tx.send(msg).await;
        }
    });

    // Send the main menu to the client
    tx.send(Message::text("Welcome to Poker!\n")).unwrap();

    // Handle the client's choice
    loop {
        tx.send(Message::text("Choose an option:\n1. Login\n2. Register\n3. Quit")).unwrap();
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

                                        // change later, get the data of the player from db
                                        let new_player = Player {
                                            name: username.clone(),
                                            id: Uuid::new_v4().to_string(),
                                            hand: Vec::new(),
                                            wallet: db.get_player_wallet(&username).await.unwrap() as i32,
                                            tx: tx.clone(),
                                            state: lobby::READY,
                                            current_bet: 0,
                                            dealer: false,
                                            ready: false,
                                            games_played: 0,
                                            games_won: 0,
                                        };

                                        lobby.add_player(new_player).await;
                                        lobby.broadcast(format!("{} has joined the lobby!", username)).await;
                                        username_id = username;
                                        println!("{} has joined the lobby!", username_id);
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
                                        tx.send(
                                            Message::text(
                                                format!("Registration successful! Welcome, {}! You are now in the lobby.", username)
                                            )
                                        ).unwrap();

                                        let new_player = Player {
                                            name: username.clone(),
                                            id: Uuid::new_v4().to_string(),
                                            hand: Vec::new(),
                                            wallet: 1000,
                                            tx: tx.clone(),
                                            state: lobby::READY,
                                            current_bet: 0,
                                            dealer: false,
                                            ready: false,
                                            games_played: 0,
                                            games_won: 0,
                                        };

                                        lobby.add_player(new_player).await;
                                        lobby.broadcast(
                                            format!("{} has joined the lobby!", username)
                                        ).await;
                                        username_id = username;
                                        println!("{} has joined the lobby!", username_id);

                                        break;
                                    }
                                    Err(_) => {
                                        tx.send(
                                            Message::text("Registration failed. Try again.")
                                        ).unwrap();
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
                        tx.send(Message::text("Ixnvalid option.")).unwrap();
                    }
                }
            }
        }
    }
    // while let Some(result) = ws_rx.next().await {
    while let Some(result) = ws_rx.next().now_or_never() {
        match result {
            Some(Ok(msg)) => {
                if msg.is_close() {
                    println!("{} has disconnected.", username_id);
                    for player in lobby.players.lock().await.iter() {
                        if player.name == username_id {
                            db.update_player_stats(&player).await.unwrap();
                            lobby.remove_player(&username_id).await;
                            lobby.broadcast(
                                format!("{} has left the lobby.", username_id)
                            ).await;
                        }
                    }
                }
            }
            Some(Err(e)) => {
                eprintln!("Error: {}", e);
            }
            None => {
                continue;
            }
        }
    }
    // loop for players join the lobby, once at least 2 have joined start the game
    loop {
        if lobby.players.lock().await.len() >= 2 {
            // 10 second delay, allow more players to join
            lobby.broadcast(format!("Enough players to start the game")).await;
            for i in (1..=10).rev() {
                lobby.broadcast(format!("Game starting in {} seconds", i)).await;
                sleep(Duration::from_secs(1)).await;
            }
            break;
        }
    }

    // change state of players in the lobby to playing
    // for player in lobby.players.lock().await.iter() {
    //     player.state = "playing".to_string();
    // }

    // start game state machine once
    tokio::spawn(async move {
        game_state_machine(&lobby).await;
    });
}
