mod database;
mod game;
mod deck;
mod lobby;

use futures_util::stream::SplitStream;
use warp::Filter;
use std::sync::Arc;
use database::Database;
use warp::ws::{Message, WebSocket};
use futures_util::{StreamExt, SinkExt};
use tokio::sync::{mpsc, Mutex};
use sqlx::SqlitePool;
use uuid::Uuid;
use lobby::*;
use deck::Deck;

const MAX_SERVER_PLAYER_COUNT: i32 = 100;

#[tokio::main]
async fn main() {
    let db_pool = SqlitePool::connect("sqlite://poker.db").await.expect(
        "Failed to connect to database"
    );

    let database = Arc::new(Database::new(db_pool.clone()));
    let server_lobby = Arc::new(Mutex::new(Lobby::new(Some(MAX_SERVER_PLAYER_COUNT), "Server Lobby".to_string()).await));
    let register_route = warp
        ::path("ws")
        .and(warp::ws())
        .and(with_db(database.clone()))
        .and(with_lobby(server_lobby.clone()))
        .map(|ws: warp::ws::Ws, db, server_lobby|
            ws.on_upgrade(move |socket| handle_connection(socket, db, server_lobby))
        );

    warp::serve(register_route).run(([0, 0, 0, 0], 1118)).await;
}

fn with_db(
    db: Arc<Database>
) -> impl Filter<Extract = (Arc<Database>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || db.clone())
}

fn with_lobby(
    lobby: Arc<Mutex<Lobby>>
) -> impl Filter<Extract = (Arc<Mutex<Lobby>>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || lobby.clone())
}

async fn get_lobby_names(server_lobby: Arc<Mutex<Lobby>>) -> String {
    let lobbies = {
        let server_lobby = server_lobby.lock().await;
        server_lobby.get_lobby_names().await
    };
    let mut lobby_list = String::from("\t");
    if lobbies.is_empty() {
        lobby_list = "No lobbies available.".to_string();
    } else {
        for lobby in lobbies {
            lobby_list.push_str(&format!("{}\t", lobby));
        }
    }
    lobby_list
}

async fn handle_connection(ws: WebSocket, db: Arc<Database>, server_lobby: Arc<Mutex<Lobby>>) {
    let (mut ws_tx, mut ws_rx) = ws.split();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut username_id = "".to_string();
    let mut current_player: Player;

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let _ = ws_tx.send(msg).await;
        }
    });

    tx.send(Message::text("Welcome to Poker!\n")).unwrap();

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

                                        let new_player = Player {
                                            name: username.clone(),
                                            id: Uuid::new_v4().to_string(),
                                            hand: Vec::new(),
                                            wallet: db.get_player_wallet(&username).await.unwrap() as i32,
                                            tx: tx.clone(),
                                            rx: Arc::new(Mutex::new(ws_rx)),
                                            state: lobby::IN_SERVER,
                                            current_bet: 0,
                                            dealer: false,
                                            ready: false,
                                            games_played: 0,
                                            games_won: 0,
                                            lobby: server_lobby.clone(),
                                        };

                                        server_lobby.lock().await.add_player(new_player.clone()).await;
                                        server_lobby.lock().await.broadcast(format!("{} has joined the server!", username)).await;
                                        
                                        username_id = username;
                                        println!("{} has joined the server!", username_id.clone());
                                        println!("server player count: {}", server_lobby.lock().await.current_player_count);
                                        println!("Server players:\n{}", server_lobby.lock().await.get_player_names().await);
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
                                            rx: Arc::new(Mutex::new(ws_rx)),
                                            state: lobby::IN_SERVER,
                                            current_bet: 0,
                                            dealer: false,
                                            ready: false,
                                            games_played: 0,
                                            games_won: 0,
                                            lobby: server_lobby.clone(),
                                        };

                                        server_lobby.lock().await.add_player(new_player.clone()).await;
                                        server_lobby.lock().await.broadcast(
                                            format!("{} has joined the server!", username)
                                        ).await;
                                        username_id = username;
                                        println!("{} has joined the server!", username_id.clone());
                                        println!("server player count: {}", server_lobby.lock().await.current_player_count);
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
                        server_lobby.lock().await.remove_player(username_id.clone()).await;
                        return;
                    }
                    _ => {
                        tx.send(Message::text("Invalid option.")).unwrap();
                    }
                }
            }
        }
    }

    let lobby_names = get_lobby_names(server_lobby.clone()).await;
    tx.send(Message::text(format!(
        "Current Lobbies:\n{}\nChoose an option:\n1. Create new lobby:            lobby -c [new lobby name]\n2. Join lobby:                  lobby -j [lobby name]\n3. Show most recent lobbies:    lobby -s\n4. View stats:                  stats\n5. Quit:                        quit\n\n",
        lobby_names
    )))
    .unwrap();

    loop {
        let result = current_player.get_player_input().await;
        match result.as_str() {
            "Disconnect" => {
                break;
            }
            "Error" => {
                eprintln!("Invalid input.");
            }
            _ => {
                match result.trim() {
                    choice if choice.starts_with("lobby -c") => {
                        let lobby_name_input = choice.split(" ").collect::<Vec<&str>>();
                        if lobby_name_input.len() != 3 {
                            tx.send(Message::text("Invalid lobby name.")).unwrap();
                            continue;
                        }
                        let lobby_name = choice.split(" ").collect::<Vec<&str>>()[2];
                        if server_lobby.lock().await.lobby_exists(lobby_name.to_string()).await {
                            tx.send(Message::text("Lobby name already exists.")).unwrap();
                        } else {
                            let new_lobby = Arc::new(Mutex::new(Lobby::new(None, lobby_name.to_string()).await));
                            server_lobby.lock().await.add_lobby(new_lobby.clone()).await;
                            server_lobby.lock().await.broadcast(
                                format!("{} has created a new lobby: {}", username_id.clone(), lobby_name)
                            ).await;
                            println!("{} has created a new lobby: {}", username_id.clone(), lobby_name);
                        }
                    }
                    choice if choice.starts_with("lobby -j") => {
                        let lobby_name_input = choice.split(" ").collect::<Vec<&str>>();
                        if lobby_name_input.len() != 3 {
                            tx.send(Message::text("Invalid lobby name.")).unwrap();
                            continue;
                        }
                        let lobby_name = choice.split(" ").collect::<Vec<&str>>()[2];
                        let join_status = current_player.player_join_lobby(server_lobby.clone(), lobby_name.to_string()).await;
                        match join_status {
                            lobby::FAILED => {
                                tx.send(Message::text("Lobby name entered not found.")).unwrap();
                            }
                            lobby::SUCCESS => {
                                server_lobby.lock().await.broadcast(format!("{} has joined lobby: {}", username_id.clone(), lobby_name)).await;
                                let exit_status = join_lobby(server_lobby.clone(), current_player.clone(), db.clone()).await;
                                current_player.state = lobby::IN_SERVER;
                                if exit_status == "Disconnect" {
                                    break;
                                }
                            }
                            lobby::SERVER_FULL => {
                                tx.send(Message::text("Lobby already full.")).unwrap();
                            }
                            _ => {
                                tx.send(Message::text("reached.")).unwrap();
                            }
                        }
                    }
                    choice if choice.starts_with("lobby -s") => {
                        let lobby_names = get_lobby_names(server_lobby.clone()).await;
                        tx.send(Message::text(format!(
                            "Current Lobbies:\n{}\n\n",
                            lobby_names
                        )))
                        .unwrap();
                    }
                    choice if choice.starts_with("stats") => {
                        
                        // VIEW STATS------------------------
                        // let stats = db.get_player_stats(&username_id).await.unwrap();
                        // tx.send(Message::text(format!("Stats: {:?}", stats))).unwrap();
                        let stats = db.player_stats(&username_id).await;
                        if let Ok(stats) = stats {
                            tx.send(Message::text(format!(
                                "Player Stats for {}: Games Played: {}, Games Won: {}, Wallet: {}",
                                &username_id, stats.games_played, stats.games_won, stats.wallet,
                            )))
                            .unwrap();
                        } else {
                            tx.send(Message::text("Failed to retrieve stats.")).unwrap();
                        }

                    }
                    choice if choice.starts_with("quit") => {
                        tx.send(Message::text("Goodbye!")).unwrap();
                        break;
                    }
                    _ => {
                        tx.send(Message::text("Invalid option.")).unwrap();
                    }
                }
            }
        }
        tx.send(Message::text(format!(
            "Current Lobbies:\n{}\nChoose an option:\n1. Create new lobby:            lobby -c [new lobby name]\n2. Join lobby:                  lobby -j [lobby name]\n3. Show most recent lobbies:    lobby -s\n4. View stats:                  stats\n5. Quit:                        quit\n\n",
            lobby_names
        )))
        .unwrap();
    }

    println!("Disconnect message ----------- Server");
    server_lobby.lock().await.remove_player(current_player.name.clone()).await;
    current_player.remove_player(server_lobby.clone(), db).await;
    println!("{} has left the server.", username_id.clone());
}

async fn join_lobby(server_lobby: Arc<Mutex<Lobby>>, mut player: Player, db: Arc<Database>) -> String {
    player.state = lobby::IN_LOBBY;
    let player_lobby = player.lobby.clone();
    let tx = player.tx.clone();
    println!("{} has joined lobby: {}", player.name, player_lobby.lock().await.name);
    let player_lobby_name = player_lobby.lock().await.name.clone();
    let player_lobby_player_count = player_lobby.lock().await.current_player_count;
    println!("lobby {} has {} players", player_lobby_name, player_lobby_player_count);

    tx.send(Message::text(format!(
        "Welcome to lobby: {}\nChoose an option:\n1. Ready:           r\n2. Show Players:    p\n3. View stats:      s\n4. Quit:            q\n\n",
        player_lobby.lock().await.name
    )))
    .unwrap();

    loop {
        let result = player.get_player_input().await;
        let lobby_state = player_lobby.lock().await.game_state.clone();
        let lobby_name = player_lobby.lock().await.name.clone();
        println!("Lobby {} state: {}", lobby_name, lobby_state);
        if player_lobby.lock().await.game_state == lobby::JOINABLE {
            match result.as_str() {
                "Disconnect" => {
                    println!("{} has disconnected ------ here.", player.name);
                    let lobby_status = player_lobby.lock().await.remove_player(player.name.clone()).await;
                    println!("lobby status: {}", lobby_status);
                    if lobby_status == lobby::GAME_LOBBY_EMPTY {
                        server_lobby.lock().await.remove_lobby(player_lobby.lock().await.name.clone()).await;
                    }
                    server_lobby.lock().await.remove_player(player.name.clone()).await;
                    player.remove_player(server_lobby.clone(), db.clone()).await;
                    if player_lobby.lock().await.game_state == lobby::JOINABLE {
                        player_lobby.lock().await.ready_up("".to_string()).await;
                    }
                    return "Disconnect".to_string();
                }
                "Error" => {
                    println!("Invalid input.");
                }
                _ => {
                    match result.trim() {
                        "p" => {
                            let players: String = player_lobby.lock().await.get_player_names().await;
                            tx.send(Message::text(format!("Players:\n{}", players)))
                                .unwrap();
                        }
                        "s" => {
                            // VIEW STATS------------------------
                            // let stats = db.get_player_stats(&username_id).await.unwrap();
                            // tx.send(Message::text(format!("Stats: {:?}", stats))).unwrap();
                            let stats = db.player_stats(&player.name).await;
                        if let Ok(stats) = stats {
                            tx.send(Message::text(format!(
                                "Player Stats for {}: Games Played: {}, Games Won: {}, Wallet: {}",
                                &player.name, stats.games_played, stats.games_won, stats.wallet,
                            )))
                            .unwrap();
                        } else {
                            tx.send(Message::text("Failed to retrieve stats.")).unwrap();
                        }
                        }
                        "q" => {
                            // QUIT LOBBY------------------------
                            let lobby_status = player_lobby.lock().await.remove_player(player.name.clone()).await;
                            println!("lobby status: {}", lobby_status);
                            if lobby_status == lobby::GAME_LOBBY_EMPTY {
                                server_lobby.lock().await.remove_lobby(player_lobby.lock().await.name.clone()).await;
                            }
                            // update player stat to DB
                            break;
                        }
                        "r" => {
                            // READY UP------------------------
                            let mut all_ready = 0;
                            player.ready = true;
                            let (ready_player_count, lobby_player_count) = player_lobby.lock().await.ready_up(player.name.clone()).await;
                            if ready_player_count == lobby_player_count {
                                all_ready = 1;
                            }
                            player_lobby.lock().await.broadcast(format!("Number of players ready: {}/{}", ready_player_count, lobby_player_count)).await;
                            println!("Number of players ready in lobby [{}]: {}/{}", player_lobby.lock().await.name, ready_player_count, lobby_player_count);
                            if lobby_player_count < 2 {
                                player_lobby.lock().await.broadcast("Need at least 2 players to start game.".to_string()).await;
                            } else if lobby_player_count >= 2 && all_ready == 1 {
                                player_lobby.lock().await.broadcast("All players ready. Starting game...".to_string()).await;
                                let player_lobby_clone = player_lobby.clone();
                                tokio::spawn(async move {
                                    let mut player_lobby_host = player_lobby_clone.lock().await;
                                    player_lobby_host.start_game().await;
                                });
                            }
                        }
                        _ => {
                            tx.send(Message::text("Invalid option.")).unwrap();
                        }
                    }
                }
            }
        } else {
            println!("Lobby in progress.");

        }
    }
    return "Normal".to_string();
}

