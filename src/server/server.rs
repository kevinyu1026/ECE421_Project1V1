use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

fn handle_client(mut stream: TcpStream) {
    let mut buffer = [0; 1024];

    // read data from stream, store in the buffer
    let bytes_read = stream.read(&mut buffer).expect("Failed to read from stream client");
    
    // convert buffer to UTF8 encodded string
    // from_utf8_lossy() is used to handle invalid UTF-8 sequences
    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    println!("Received: {}", request);

    // TODO: parse/handle request
    // then based off this request, we can handle bet, check, fold, call, raise, etc.
    if request.trim() == "Join" {
        stream.write("Player joined the game!".as_bytes()).expect("Failed to write to stream");        
        // then make a new player object and add to the game state
    }


    // write a response to client stream
    // let response = "";
    // stream.write(response.as_bytes()).unwrap();
}

pub fn main() {
    // bind server to localhost:3333
    let listener = TcpListener::bind("127.0.0.1:3334").expect("Failed to bind address");
    // create a new game state reference -> as long as theres 1 reference to this, it will be alive
    // let game_state = Arc::clone(&game_state);

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                // let thread_game_state = Arc::clone(&game_state);
                // spawn a new thread for each client
                std::thread::spawn(|| handle_client(stream));
            }
            Err(e) => {
                eprintln!("Failed to establish a connection: {}", e);
            }
        }
    }
}

// each server thread should have access to this
// each thread will update this after every turn
struct GameState {
    // TODO
    // deck, players, store player hands, player bets, pot, etc.
}

impl GameState {
    // method to create a new game
    fn new() -> GameState {
        GameState {
            // TODO
        }
    }
}