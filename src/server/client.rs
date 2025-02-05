use std::io::{ Read, Write };
use std::net::{ TcpListener, TcpStream };

struct GameMessage {
    action: String,
    amount: i32,
}

fn main() {
    let mut stream = TcpStream::connect("127.0.0.1:3334").unwrap();
    println!("Connected to the server!");

    let mut message = GameMessage {
        action: "Join".to_string(),
        amount: 0,
    };
    stream.write(&message.action.as_bytes()).expect("Failed to write to stream");

    let mut buffer = [0; 1024];
    // read data from stream, store in the buffer -> thinking while loop to continue doing this until connection closed??

    stream.read(&mut buffer).expect("Failed to read from stream client");
    let response = String::from_utf8_lossy(&buffer[..]);
    println!("Received: {}", response);
}
