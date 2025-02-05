
use warp::ws::Message;
use tokio::sync::mpsc::UnboundedSender;

pub async fn show_game_variants(tx: &UnboundedSender<Message>) {
    let game_menu = Message::text(
        "Select a Poker Variant:\n1. Texas Hold'em\n2. Five-Card Draw\n3. Badugi"
    );
    let _ = tx.send(game_menu);
}

pub async fn handle_game_selection(choice: &str, tx: &UnboundedSender<Message>) {
    loop {
        match choice {
            "1" => {
                let msg = Message::text("You have selected Texas Hold'em.");
                let _ = tx.send(msg);
                // Call Texas Hold'em game logic here
                break;
            }
            "2" => {
                let msg = Message::text("You have selected Five-Card Draw.");
                let _ = tx.send(msg);
                // Call Five-Card Draw game logic here
                break;
            }
            "3" => {
                let msg = Message::text("You have selected Badugi.");
                let _ = tx.send(msg);
                // Call Badugi game logic here
                break;
            }
            _ => {
                let msg = Message::text("Invalid selection. Please choose 1, 2, or 3.");
                let _ = tx.send(msg);
            }
        }
    }
}
