use std::collections::hash_map::{Entry,HashMap};
use async_std::sync::{Arc, RwLock};
use tide::{Body, Request};
use tide_websockets::{Message as WSMessage, WebSocket, WebSocketConnection};
use futures_util::StreamExt;
use petname::Petnames;
use serde_derive::{Serialize,Deserialize};
use serde_json::json;

#[derive(Clone)]
struct Player {
    id: PlayerId, // connection Id
    wsc : WebSocketConnection,
    label : String
}
#[derive(Clone,Serialize,Deserialize,PartialEq,Eq)]
struct PlayerId {
    id: Option<String>,
}

impl Default for PlayerId {
    fn default() -> Self {
        Self {
            id: None
        }
    }
}
#[derive(Clone)]
struct Board {
    id: String, // id of the board
    play_book: [String;9],
    players: Vec<Player>
}

#[derive(Clone, Debug, Serialize)]
struct GameCommand {
    cmd: String,
    play_book: [String;9]
}

#[derive(Clone)]
struct State {
    boards: Arc<RwLock<HashMap<String,Board>>>,
}

impl State {
    fn new() -> Self {
        Self {
            boards: Default::default(),
        }
    }

    async fn add_player_to_board(&self, board_id: &str, mut player: Player ) -> Result<String,String> {
        let mut boards = self.boards.write().await;
        match boards.entry(board_id.to_owned()) {
            Entry::Vacant(_) => {
                player.label = String::from('X');
                let b = Board {
                    id : board_id.to_owned(),
                    play_book : Default::default(),
                    players: vec![player]
                };
                boards.insert(board_id.to_owned(), b );
                Ok(String::from('X'))
            },
            Entry::Occupied(mut board) => {
                // check if we had the two players
                let mut players = board.get_mut().players.clone();

                // check if already in the board
                let p = players.clone().into_iter().filter(|x| {
                    x.id == player.id
                 }).collect::<Vec<Player>>();

                if p.len() == 1 {
                    let label = p[0].label.clone();
                    board.get_mut().players = players;
                    return Ok(label)
                }

                // check if we can add to the board
                if players.len() < 2 {
                    let other_player = &players[0];
                    player.label = if other_player.label == "X" { String::from("O")} else { String::from("X") };

                    let label = player.label.clone();
                    players.push( player );
                    board.get_mut().players = players;
                    Ok(label)
                } else {
                    return Err(String::from("COMPLETE"))
                }
            }
        }
    }

    async fn make_play_in_board(&self, board_id: &str, player_label: String,  cell_index: usize) -> tide::Result<()> {
        let mut boards = self.boards.write().await;
        let mut board = boards.get_mut(board_id).unwrap();
        board.play_book[cell_index] = player_label;

        Ok(())
    }

    async fn send_message(&self, board_id: &str, message: GameCommand) -> tide::Result<()> {
        let mut boards = self.boards.write().await;
        match boards.entry(board_id.to_owned()) {
            Entry::Vacant(_) => {
                println!("{} vacant", board_id);
            },
            Entry::Occupied(mut board) => {
                println!("sending state to board {}", board_id);
                for player in &board.get_mut().players {
                    println!("{} message {} - player: {}", board_id, message.cmd, player.label);
                    player.wsc.send_json(&json!({
                        "cmd": message.cmd,
                        "play_book" : message.play_book
                    })).await?
                }
            }
        }

        Ok(())
    }

    async fn reset_board( &self, board_id: &str) -> tide::Result<()> {
        let mut boards = self.boards.write().await;
        let mut board = boards.get_mut(board_id).unwrap();


        board.play_book = Default::default();

        Ok(())
    }

    async fn leave_board( &self, board_id: &str, player_id: PlayerId) -> tide::Result<()> {
        let mut boards = self.boards.write().await;
        let mut board = boards.get_mut(board_id).unwrap();

        let p = board.players.clone().into_iter().filter(|x| {
            x.id != player_id
         }).collect::<Vec<Player>>();
        board.players = p;

        Ok(())
    }

}

#[async_std::main]
async fn main() -> Result<(), std::io::Error> {
    env_logger::init();

    // let mut app = tide::new();
    let mut app = tide::with_state(State::new());

    // serve public dir for assets
    app.at("/public").serve_dir("./public/")?;

    // index route
    app.at("/").get(|_| async { Ok(Body::from_file("./public/index.html").await?) });

    // new route
    app.at("/new").post(|_| async {
        let petnames = Petnames::default();
        let board_name = petnames.generate_one(2, "-");
        Ok( json!({ "board_name" :board_name}))
    });

    // board route
    app.at("/:id")
            .with(WebSocket::new(
                |req: Request<State>, mut wsc: WebSocketConnection| async move {
                    let board_id = req.param("id")?;
                    let client: PlayerId  = req.query().unwrap_or_default();
                    let state = req.state().clone();

                    let petnames = Petnames::default();
                    let player_id = match client.id {
                        Some( id ) => id,
                        None => petnames.generate_one(2, ".")
                    };

                    let player = Player {
                        id :  PlayerId {id : Some(player_id.clone())},
                        wsc : wsc.clone(),
                        label: String::from("")
                    };

                    match state.add_player_to_board(board_id, player).await {
                        Ok( player_label ) => {
                            let boards = state.boards.read().await;
                            wsc.send_json(&json!({
                                "cmd":"INIT",
                                "player":player_label,
                                "play_book" : boards.get(board_id).unwrap().play_book.clone(),
                                "client_id" : player_id
                            })).await?
                        }
                        Err(_) => {
                            wsc.send_json(&json!({
                                "cmd":"COMPLETE"
                            })).await?
                        }
                    }

                while let Some(Ok(WSMessage::Text(message))) = wsc.next().await {
                    println!("{:?}", message);
                    let parts: Vec<&str> = message.split(":").collect();

                    match parts[0] {
                        "PLAY" => {
                            state.make_play_in_board(board_id, parts[1].parse().unwrap(), parts[2].parse().unwrap()).await?;
                            let boards = state.boards.read().await;
                            let play_book = boards.get(board_id).unwrap().play_book.clone();

                            // needs to release the lock here since `send_message` needs to access the board.
                            drop(boards);

                            let cmd = String::from("STATE");
                            state.send_message(board_id, GameCommand { cmd, play_book }).await?;
                        },
                        "RESET" => {
                            state.reset_board(board_id).await?;
                            let cmd = String::from("RESET");
                            state.send_message(board_id, GameCommand{ cmd, play_book : Default::default() }).await?;
                        },
                        "LEAVE" => {
                            state.leave_board(board_id, PlayerId {id : Some(player_id.clone())}).await?;
                            let boards = state.boards.read().await;
                            let play_book = boards.get(board_id).unwrap().play_book.clone();

                            // needs to release the lock here since `send_message` needs to access the board.
                            drop(boards);

                            let cmd = String::from("LEAVE");
                            state.send_message(board_id, GameCommand{ cmd, play_book }).await?;

                        }
                        _ => println!( "INVALID message")
                    }
                }

                Ok(())
            },
        ))
    .get(|_| async { Ok(Body::from_file("./public/board.html").await?) });

    // random
    app.at("/random").post(|request: Request<State>| async move {
        let mut board_name = String::from("");
        let state = request.state().clone();
        let mut boards = state.boards.write().await;

        let board = boards.iter_mut().find(|x| {
           x.1.players.len() == 1
        });

        if let Some(b) = board {
            board_name = b.0.to_owned()
        }

        drop(boards);

        println!("board_name : {}", board_name);
        Ok( json!({ "board_name" :board_name}))
    });

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{}", port);
    app.listen(addr).await?;

    Ok(())
}