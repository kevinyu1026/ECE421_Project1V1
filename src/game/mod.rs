use crate::Lobby;
use crate::Card;
use tokio::sync::mpsc;
use warp::ws::Message;


pub struct Player{
    name: String,
    id: String,
    hand: Vec<Card>,
    cash: i32,
    tx: mpsc::UnboundedSender<Message>,
    ingame_state: String,
    current_bet: i32,
    dealer: bool,
    ready: bool,
    
}

impl Player {
    fn new(name: String, id: String, tx: mpsc::UnboundedSender<Message>) -> Player {
        Player {
            name,
            id,
            hand: Vec::new(),
            cash: 1000,
            tx,
            ingame_state: "pregame".to_string(),
            current_bet: 0,
            dealer: false,
            ready: false,
        }
    }
    // implementation of the action
    
}
pub async fn game_state_machine(Lobby: &Lobby) {
        // begin game loop
        let mut state = "pregame";
        loop {
            match state {
                "pregame" => {

                state = "next_state"; // or any other state transition
            },
            _ => {
                println!("Invalid state");
            }
        }
    }
    }