use std::collections::hash_map::{Entry,HashMap};
use async_std::sync::{Arc, RwLock};
use broadcaster::BroadcastChannel;
use futures_util::future::Either;
use futures_util::StreamExt;
use serde_derive::Serialize;
use serde_json::json;
use tide::http::format_err;
use tide::Body;
use tide::Request;
// use tide_websockets::{WebSocketConnection, async_tungstenite::tungstenite::{Message as WSMessage}};
// use tide_websockets::WebsocketMiddleware;
use tide_websockets::{Message as WSMessage, WebSocket, WebSocketConnection};

#[derive(Clone, Debug, Serialize)]
enum Message {
    Command { cmd: String, play_book: [String;9] }
}

#[derive(Clone)]
struct Board {
    id: String, // id of the board
    play_book: [String;9], // Arc<RwLock<[String;9]>>,
    wscs: Vec<WebSocketConnection> //BroadcastChannel<Message>
}


#[derive(Clone)]
struct State {
    broadcaster: BroadcastChannel<Message>,
    boards: Arc<RwLock<HashMap<String,Board>>>,
    play_book: Arc<RwLock<[String;9]>>,
}

impl State {
    fn new() -> Self {
        let plays:[String;9] = Default::default();
        Self {
            broadcaster: BroadcastChannel::new(),
            boards: Default::default(),
            play_book: Arc::new(RwLock::new(plays)),
        }
    }

    async fn add_player_to_board(&self, board_id: &str, wsc: WebSocketConnection ) -> Result<(usize,[String;9]),String> { // tide::Result<()> {
        let mut boards = self.boards.write().await;
        match boards.entry(board_id.to_owned()) {
            Entry::Vacant(_) => {
                let b = Board {
                    id : board_id.to_owned(),
                    play_book : Default::default(),
                    wscs : vec![wsc],
                };
                boards.insert(board_id.to_owned(), b );
                Ok((0,boards.get(board_id).unwrap().play_book.clone()))
            },
            Entry::Occupied(mut board) => {
                // check if we had the two players
                let mut b = board.get_mut().wscs.clone();
                if b.len() < 2 {
                    b.push( wsc );
                    board.get_mut().wscs = b;
                    Ok((1,board.get().play_book.clone()))
                } else {
                    return Err(String::from("COMPLETE"))
                }
            }
        }
    }

    async fn make_play_in_board(&self, board_id: &str, player_label: String,  cell_index: usize) -> tide::Result<()> {

        let mut boards = self.boards.write().await;
        let mut board = boards.get_mut(board_id).unwrap();
        let mut pb = board.play_book.clone();
        pb[cell_index] = player_label;

        board.play_book = pb.clone();

        drop(boards);

        let cmd = String::from("STATE");
        println!("{} to send state", board_id);
        self.send_message(board_id, Message::Command{ cmd, play_book : pb }).await
    }

    async fn reset_board( &self, board_id: &str) -> tide::Result<()> {
        let mut boards = self.boards.write().await;
        let mut board = boards.get_mut(board_id).unwrap();


        board.play_book = Default::default();

        drop(boards);

        let cmd = String::from("RESET");
        self.send_message(board_id, Message::Command{ cmd, play_book : Default::default() }).await
    }


    async fn send_message(&self, board_id: &str, message: Message) -> tide::Result<()> {
        let mut boards = self.boards.write().await;
        match boards.entry(board_id.to_owned()) {
            Entry::Vacant(_) => {
                println!("{} vacant", board_id);
            },
            Entry::Occupied(mut board) => {

                match message {
                    Message::Command { cmd, play_book } => {
                        println!("{} messages", board_id);
                        for wsc in &board.get_mut().wscs {
                            println!("{} message {}", board_id, cmd);
                            wsc.send_json(&json!({
                                "cmd": cmd,
                                "play_book" : play_book.clone()
                            })).await?
                        }
                    }
                };
            }
        }

        Ok(())
    }
}

#[async_std::main]
async fn main() -> Result<(), std::io::Error> {
    dotenv::dotenv().ok();
    env_logger::init();

    let mut app = tide::with_state(State::new());

    let mut state = app.state().clone();
    async_std::task::spawn(async move {
        while let Some(message) = state.broadcaster.next().await {
            match message {
                Message::Command { cmd, play_book } => println!("Cmd - {}: {:?}", cmd, play_book),
            };
        }
        tide::Result::Ok(())
    });

    app.at("/:id")
        // .with(WebsocketMiddleware::new(
        .with(WebSocket::new(
            |request: Request<State>, wsc| async move {
                let key = request.param("id")?;

                let state = request.state().clone();
                let broadcaster = state.broadcaster.clone();

                let mut combined_stream = futures_util::stream::select(
                    wsc.clone().map(|l| Either::Left(l)),
                    broadcaster.clone().map(|r| Either::Right(r)),
                );


                match state.add_player_to_board(key, wsc.clone()).await {
                    Ok( ( player, play_book) ) => {
                        wsc.send_json(&json!({
                            "cmd":"INIT",
                            "player":player,
                            "play_book" : play_book.clone()
                        })).await?
                    }
                    Err(_) => {
                        wsc.send_json(&json!({
                            "cmd":"COMPLETE"
                        })).await?
                    }
                }

                while let Some(item) = combined_stream.next().await {
                    match item {
                        Either::Left(Ok(WSMessage::Text(message))) => {
                            println!("{:?}", message);
                            let parts: Vec<&str> = message.split(":").collect();

                            match parts[0] {
                                "PLAY" => {
                                    state.make_play_in_board(key, parts[1].parse().unwrap(), parts[2].parse().unwrap()).await?;
                                },
                                "RESET" => {
                                    state.reset_board(key).await?;
                                },
                                "LEAVE" => {
                                    // state.leave_board(key).await?;
                                }
                                _ => println!( "INVALID message")
                            }
                            // if parts[0] == "LEAVE" {
                            //     // state.remove_player(player_index.unwrap()).await?;
                            // } else if parts[0] == "PLAY" {
                            //     state.make_play(parts[1].parse().unwrap(), parts[2].parse().unwrap(), key).await?;
                            // } else {
                            //     println!( "INVALID message")
                            // };
                        }


                        Either::Right(Message::Command { cmd, play_book }) => {
                            wsc.send_json(
                                &json!({ "cmd" : cmd, "play_book": play_book }),
                            )
                            .await?;
                        }

                        o => {
                            log::debug!("{:?}", o);
                            return Err(format_err!("no idea"));
                        }
                    }
                }

                Ok(())
            },
        ))
    .get(|_| async { Ok(Body::from_file("./public/game.html").await?) });

    app.at("/new").post(|_| async {
        let petnames = petname::Petnames::default();
        let board_name = petnames.generate_one(2, "-");
        Ok( json!({ "board_name" :board_name}))
    });

    app.at("/random").post(|request: Request<State>| async move {
        let mut board_name = String::from("");
        let state = request.state().clone();
        let mut boards = state.boards.write().await;

        let board = boards.iter_mut().find(|x| {
           x.1.wscs.len() == 1
        });

        if let Some(b) = board {
            board_name = b.0.to_owned()
        }

        println!("board_name : {}", board_name);
        Ok( json!({ "board_name" :board_name}))
    });

    app.at("/").get(|_| async { Ok(Body::from_file("./public/index.html").await?) });

    app.at("/public").serve_dir("./public/")?;

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{}", port);
    app.listen(addr).await?;

    Ok(())
}