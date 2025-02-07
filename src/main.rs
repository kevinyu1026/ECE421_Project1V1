mod database;
mod game;
mod deck;
mod lobby;

use futures_util::stream::SplitStream;
// use std::net::TcpStream;
// use tokio_tungstenite::WebSocketStream;
// use tokio_tungstenite::tungstenite::handshake::server;
use warp::Filter;
use std::sync::Arc;
use database::Database;
use warp::ws::{ Message, WebSocket };
use futures_util::{ StreamExt, SinkExt };
use tokio::sync::mpsc;
use sqlx::SqlitePool;
use uuid::Uuid;
use lobby::*;
use deck::Deck;
// use tokio::time::{ sleep, Duration };

const MAX_SERVER_PLAYER_COUNT: i32 = 100;

// Main function to start the server
#[tokio::main]
async fn main() {
    let db_pool = SqlitePool::connect("sqlite://poker.db").await.expect(
        "Failed to connect to database"
    );

    let database = Arc::new(Database::new(db_pool.clone()));
    let server_lobby = Arc::new(Lobby::new(Some(MAX_SERVER_PLAYER_COUNT), "Server Lobby".to_string()).await);
    let register_route = warp
        ::path("ws")
        .and(warp::ws())
        .and(with_db(database.clone()))
        .and(with_lobby(server_lobby.clone()))
        .map(|ws: warp::ws::Ws, db, server_lobby|
            ws.on_upgrade(move |socket| handle_connection(socket, db, server_lobby))
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

async fn get_lobby_names(server_lobby:Lobby) -> String {
    let lobbies = server_lobby.get_lobby_names().await;
    let mut lobby_list = String::from("\t");
    if lobby_list.is_empty() {
        lobby_list = "No lobbies available.".to_string();
    } else {
        for lobby in lobbies {
            lobby_list.push_str(&format!("{}\t", lobby));
        }
    }
    lobby_list
}


// Handle the WebSocket connection
async fn handle_connection(ws: WebSocket, db: Arc<Database>, server_lobby: Arc<Lobby>) {
    
    let mut server_lobby = Arc::try_unwrap(server_lobby).unwrap_or_else(|arc| (*arc).clone());
    let (mut ws_tx, mut ws_rx) = ws.split();
    // let (mut ws_lobby_tx, mut ws_lobby_rx) = ws.split();

    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut username_id = "".to_string(); // saving so we can match the player to the player object
    let current_player:Player;
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
                                            rx: Arc::new(tokio::sync::Mutex::new(ws_rx)),
                                            state: lobby::IN_SERVER,
                                            current_bet: 0,
                                            dealer: false,
                                            ready: false,
                                            games_played: 0,
                                            games_won: 0,
                                            lobby: server_lobby.clone(),
                                        };

                                        server_lobby.add_player(new_player.clone()).await;
                                        server_lobby.broadcast(format!("{} has joined the server!", username)).await;
                                        username_id = username;
                                        println!("{} has joined the server!", username_id.clone());
                                        current_player = new_player;
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
                                                format!("Registration successful! Welcome, {}! You are now in the Server.", username)
                                            )
                                        ).unwrap();
                                        let new_player = Player {
                                            name: username.clone(),
                                            id: Uuid::new_v4().to_string(),
                                            hand: Vec::new(),
                                            wallet: 1000,
                                            tx: tx.clone(),
                                            rx: Arc::new(tokio::sync::Mutex::new(ws_rx)),
                                            state: lobby::IN_SERVER,
                                            current_bet: 0,
                                            dealer: false,
                                            ready: false,
                                            games_played: 0,
                                            games_won: 0,
                                            lobby: server_lobby.clone(),
                                        };

                                        server_lobby.add_player(new_player.clone()).await;
                                        server_lobby.broadcast(
                                            format!("{} has joined the server!", username)
                                        ).await;
                                        username_id = username;
                                        println!("{} has joined the server!", username_id.clone());
                                        current_player = new_player;
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
                        server_lobby.remove_player(server_lobby.clone(), username_id.clone()).await;
                        return;
                    }
                    _ => {
                        tx.send(Message::text("Invalid option.")).unwrap();
                    }
                }
            }
        }
    }
    // join player to server player list as Player object------------
    // print out player options in server lobby---------
    let lobby_names = get_lobby_names(server_lobby.clone()).await;
    tx.send(Message::text(format!("
        Current Lobbies:\n
        {}\n
        Choose an option:\n
        1. Create new lobby:            lobby -c [new lobby name]\n
        2. Join lobby:                  lobby -j [lobby name]\n
        3. Show most recent lobbies:    lobby -s\n
        4. View stats:                  stats\n
        5. Quit:                        quit\n\n
    ", lobby_names))).unwrap();
    loop {
        let result = current_player.get_player_input(username_id.clone()).await;
        match result.as_str() {
            "Disconnected" => {
                server_lobby.remove_player(server_lobby.clone(), current_player.name.clone()).await;
                current_player.clone().remove_player(server_lobby.clone(), db).await;
                println!("{} has left the server.", username_id.clone());
                break;
            }
            "Error" => {
                eprintln!("Invalid input.");
            }
            _ => {
                // handles client response here----------------
                let choice = result.trim();


                if choice.starts_with("lobby -c") {
                    // CREATE LOBBY - return lobby object----------------
                    let lobby_name_input = choice.split(" ").collect::<Vec<&str>>();
                    if lobby_name_input.len() != 3 {
                        tx.send(Message::text("Invalid lobby name.")).unwrap();
                        continue;
                    }
                    let lobby_name = choice.split(" ").collect::<Vec<&str>>()[2];
                    if server_lobby.lobby_exists(lobby_name.to_string()).await {
                        tx.send(Message::text("Lobby name already exists.")).unwrap();
                    } else {
                        let new_lobby = Lobby::new(None, lobby_name.to_string()).await;
                        server_lobby.add_lobby(new_lobby.clone()).await;
                        server_lobby.broadcast(
                            format!("{} has created a new lobby: {}", username_id.clone(), lobby_name)
                        ).await;
                        println!("{} has created a new lobby: {}", username_id.clone(), lobby_name);
                    }
                    let join_status = server_lobby.player_join_lobby(username_id.clone(), lobby_name.to_string()).await;
                    if join_status == lobby::FAILED {
                        tx.send(Message::text("Lobby name entered not found.")).unwrap();
                    } else if join_status == lobby::SUCCESS {
                        server_lobby.broadcast(format!("{} has joined lobby: {}", username_id.clone(), lobby_name)).await;
                        join_lobby(server_lobby.clone(), current_player.clone(), db.clone()).await; // Sync function for player to be inside until they leave lobby or disconnect from server
                    } else if join_status == lobby::SERVER_FULL {
                        tx.send(Message::text("Lobby already full.")).unwrap();
                    } else {
                        tx.send(Message::text("reached.")).unwrap();
                    }


                } else if choice.starts_with("lobby -j") {
                    // JOINING LOBBY (EITHER EXITING OR NEWLY CREATED)---------------
                    let lobby_name_input = choice.split(" ").collect::<Vec<&str>>();
                    if lobby_name_input.len() != 3 {
                        tx.send(Message::text("Invalid lobby name.")).unwrap();
                        continue;
                    }
                    let lobby_name = choice.split(" ").collect::<Vec<&str>>()[2];
                    let join_status = server_lobby.player_join_lobby(username_id.clone(), lobby_name.to_string()).await;
                    if join_status == lobby::FAILED {
                        tx.send(Message::text("Lobby name entered not found.")).unwrap();
                    } else if join_status == lobby::SUCCESS {
                        server_lobby.broadcast(format!("{} has joined lobby: {}", username_id.clone(), lobby_name)).await;
                        join_lobby(server_lobby.clone(), current_player.clone(), db.clone()).await; // Sync function for player to be inside until they leave lobby or disconnect from server
                    } else if join_status == lobby::SERVER_FULL {
                        tx.send(Message::text("Lobby already full.")).unwrap();
                    } else {
                        tx.send(Message::text("reached.")).unwrap();
                    }


                } else if choice.starts_with("lobby -s") {
                    // SHOW EXISTING LOBBIES------------------------
                    let lobby_names = get_lobby_names(server_lobby.clone()).await;
                    tx.send(Message::text(format!("
                        Current Lobbies:\n
                        {}\n\n
                    ", lobby_names))).unwrap();


                } else if choice.starts_with("stats") {
                    // VIEW STATS------------------------
                    // let stats = db.get_player_stats(&username_id).await.unwrap();
                    // tx.send(Message::text(format!("Stats: {:?}", stats))).unwrap();


                } else if choice.starts_with("quit") {
                    tx.send(Message::text("Goodbye!")).unwrap();
                    println!("{} has left the server.", username_id.clone());
                    for player in server_lobby.players.lock().await.iter() {
                        if player.name == username_id {
                            db.update_player_stats(&player).await.unwrap();
                            server_lobby.clone().remove_player(server_lobby.clone(), username_id.clone()).await;
                            server_lobby.broadcast(
                                format!("{} has left the server.", username_id.clone())
                            ).await;
                        }
                    }
                    return;


                } else {
                    tx.send(Message::text("Invalid option.")).unwrap();
                }
            }
            // print options again
        }
    }
}


async fn join_lobby(server_lobby: Lobby, mut player: Player, db: Arc<Database>) {
    let mut player_lobby = player.lobby.clone();
    let tx = player.tx.clone();
    tx.send(Message::text(format!(
        "
        Welcome to lobby: {}\n
        Choose an option:\n
        1. Ready:           r\n
        2. Show Players:    p\n
        3. View stats:      s\n
        4. Quit:            q\n\n
    ",
        player_lobby.name
    )))
    .unwrap();

    loop {
        let result = player.get_player_input(player.name.clone()).await;
        if player.state == lobby::IN_LOBBY {
            continue;
        }
        match result.as_str() {
            "Disconnect" => {
                player_lobby.remove_player(server_lobby.clone(), player.name.clone()).await;
                player.remove_player(server_lobby.clone(), db.clone()).await;
                println!("{} has disconnected.", player.name);
                if player_lobby.game_state == lobby::JOINABLE {
                    player_lobby.ready_up("".to_string()).await;
                } else {
                    player.state = lobby::IN_SERVER;
                    // update player stat to DB
                    player_lobby.remove_player(server_lobby.clone(), player.name.clone()).await;
                }
                break;
            }
            "Error" => {}
            _ => {
                let choice = result.trim();
                if choice.starts_with("r") {
                    // READY UP------------------------
                    player.ready = true;
                    player_lobby.ready_up(player.name.clone()).await;
                    // if all players are ready, start game
                    let players = player_lobby.players.lock().await;
                    if players.iter().all(|p| p.ready) {
                        // player_lobby.start_game().await;
                    }
                } else if choice.starts_with("p") {
                    let players: String = player_lobby.get_player_names().await;
                    tx.send(Message::text(format!("Players: {}", players)))
                        .unwrap();
                } else if choice.starts_with("s") {
                    // VIEW STATS------------------------
                    // let stats = db.get_player_stats(&username_id).await.unwrap();
                    // tx.send(Message::text(format!("Stats: {:?}", stats))).unwrap();
                } else if choice.starts_with("q") {
                    // QUIT LOBBY------------------------
                    player_lobby
                        .remove_player(server_lobby.clone(), player.name.clone())
                        .await;
                    println!("{} has left the lobby.", player.name);
                    player_lobby.remove_player(server_lobby.clone(), player.name.clone()).await;
                    player.state = lobby::IN_SERVER;
                    // update player stat to DB
                    break;
                } else {
                    tx.send(Message::text("Invalid option.")).unwrap();
                }
            }
        }
    }
} // return back to handle_connection()

