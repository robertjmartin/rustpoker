#![allow(unused)] // remove before commit
use axum::{
    extract::ws::{WebSocket, WebSocketUpgrade, Message},
    extract::{State, Path},
    response::{Html, Response, IntoResponse},
    routing::{get, post},
    Router,
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::{Uuid};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::select;
use tokio::sync::mpsc;
use tokio::time::{self, Duration, MissedTickBehavior};
use pokercore::{Table, GameEvent, TableEvent};

mod client_command;
mod client_responses;
mod table_command;
mod wire;
pub use client_command::{*};
pub use client_responses::{*};
pub use table_command::{*};
pub use wire::{*};

const CLIENT_TEST_HTML: &str = include_str!("../static/test.html");

#[derive(Debug, Clone, Serialize)]
struct User {
    id: UserId,
    name: String,
    balance: u32,
}

#[derive(Clone)]
struct AppState {
    users: Arc<Mutex<HashMap<UserId, User>>>,
    table_tx: mpsc::Sender<TableCommand>,
}

// The long-running task. It exits when all Senders are dropped.
async fn worker(mut rx: mpsc::Receiver<TableCommand>) {
    let mut table = Table::new(8);
    let mut timer = time::interval(Duration::from_secs(5));
    timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut connections: HashMap<UserId, mpsc::Sender<GameEvent>> = HashMap::new();
    let mut seat_map: HashMap<UserId, u8> = HashMap::new();

    loop {
        flush_events(&mut table, &connections, &seat_map).await;

        select! {
            recv = rx.recv() => {
                match recv {
                    Some(table_command) => {
                        match table_command.cmd {
                            TableCommands::Join(join_cmd) => {
                                connections.insert(table_command.user_id, join_cmd.tx);
                            },
                            TableCommands::Leave => {},
                            TableCommands::Action(table_action) => {
                                match table_action.action {
                                    TableAction::Sit(sit_action) => {
                                        match table.sit(sit_action.bank_amount) {
                                            Some(x) => {
                                                seat_map.insert(table_command.user_id, x);
                                            },
                                            None => {},
                                        }
                                    },
                                    TableAction::Stand => {},
                                    TableAction::Bet(bet_action) => {},
                                    TableAction::Fold => {},
                                }
                            },
                        }
                    },
                    None => break,                    
                }
            }
            _ = timer.tick() => {
                table.process();
                for c in &connections {
                    c.1.send(GameEvent::TableEvent(TableEvent::DummyEvent)).await;
                    // todo catch Err and remove dead connections
                }
            }
        }
    }
}

// Fan a batch of queued core events out to every connection at the table.
async fn flush_events(
    table: &mut Table,
    connections: &HashMap<UserId, mpsc::Sender<GameEvent>>,
    seat_map: &HashMap<UserId, u8>,
) {
    for event in table.drain_events() {
        match &event {
            GameEvent::TableEvent(_) => {
                for c in connections {
                    if c.1.send(event.clone()).await.is_err() {
                        // todo catch Err and remove dead connections
                    }
                }
            }
            GameEvent::HoleCardEvent(hole_cards) => {
                let seat = hole_cards.seat.value();
                for (user_id, user_seat) in seat_map {
                    if *user_seat != seat {
                        continue;
                    }
                    if let Some(tx) = connections.get(user_id) {
                        if tx.send(event.clone()).await.is_err() {
                            // todo catch Err and remove dead connections
                        }
                    }
                }
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let (table_tx, table_rx) = mpsc::channel(100);
    tokio::spawn(worker(table_rx));
    
    let state = AppState {
        users: Arc::new(Mutex::new(HashMap::new())),  //todo consider RwLock
        table_tx,
    };

    let app = Router::new()
        .route("/test", get(|| async {
            ([(axum::http::header::CACHE_CONTROL, "no-store")], Html(CLIENT_TEST_HTML))
        }))
        .route("/ping", get(|| async { "{\"status\": \"ok\"}" }))
        .route("/ws", get(ws_handler))
        .route("/register", post(handle_register))
        .route("/user/{id}" , get(handle_getuser))
        .route("/usercount", get(handle_usercount))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();
    println!("listening on {}", listener.local_addr().unwrap());

    axum::serve(listener, app).await.unwrap();
}

#[derive(Deserialize)]
struct HandleRegisterBody {
    name: String,
}

async fn handle_register(State(state): State<AppState>, Json(payload): Json<HandleRegisterBody>) -> (StatusCode, Json<User>)  {
    let new_id = client_command::UserId(uuid::Uuid::new_v4());
    
    let new_user = User {
        id: new_id,
        name: payload.name,
        balance: 1000,
    };

    state.users.lock().unwrap().insert(new_id, new_user.clone());
    
    (StatusCode::CREATED, Json(new_user))
}

async fn handle_getuser(State(state): State<AppState>, Path(id): Path<UserId>) -> Response {
    match state.users.lock().unwrap().get(&id) {
        Some(u) => Json(u.clone()).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[derive(Serialize)]
struct HandleUserCountResult {
    count: usize,
}

async fn handle_usercount(State(state): State<AppState>) -> (StatusCode, Json<HandleUserCountResult>) {
    let result = HandleUserCountResult {
        count: state.users.lock().unwrap().len(),
    };
    
    (StatusCode::OK, Json(result))
}

async fn ws_handler(State(state): State<AppState>, ws: WebSocketUpgrade) -> impl IntoResponse {
   ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn process_client_command(state: &AppState, auth_user_id: &mut Option<UserId>, tx: &mpsc::Sender<GameEvent>, cmd: ClientCommand) -> ClientResponse {
    match cmd {
        ClientCommand::SetAuthentication(cmd) => {
            let users = state.users.lock().unwrap();
            if users.contains_key(&cmd.user_id) {
                *auth_user_id = Some(cmd.user_id);
                ClientResponse::Success(ClientCommandResponse::SetAuthentication(AuthenticationContent {user_id: cmd.user_id}))
            } else {
                ClientResponse::Error(ClientCommandError::Unknown)
            }       
        },
        ClientCommand::GetAuthentication => match auth_user_id {
            Some(user_id) => ClientResponse::Success(ClientCommandResponse::SetAuthentication(AuthenticationContent {user_id: *user_id})),
            None => ClientResponse::Error(ClientCommandError::Unknown),
        },
        ClientCommand::JoinTable(cmd) => {
            match auth_user_id {
                Some(user_id) => {
                    state.table_tx.send(TableCommand {user_id: *user_id, cmd: TableCommands::Join(JoinTableCommand{tx: tx.clone()})} ).await;
                    ClientResponse::Success(ClientCommandResponse::JoinTable)
                },
                None => ClientResponse::Error(ClientCommandError::NotAuthenticated), 
            }
        },
        ClientCommand::LeaveTable => ClientResponse::Success(ClientCommandResponse::LeaveTable),
        ClientCommand::Table(cmd) => {
            match auth_user_id {
                Some(user_id) => {
                    //todo handle cmd.table_id
                    state.table_tx.send(TableCommand {user_id: *user_id, cmd: TableCommands::Action(ActionTableCommand {action: cmd.action})}).await;
                    ClientResponse::Success(ClientCommandResponse::Table)
                },
                None => ClientResponse::Error(ClientCommandError::NotAuthenticated), 
            }
        }
    }
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let (tx, mut rx) = mpsc::channel::<GameEvent>(100);
    let mut auth_user_id: Option<UserId> = None;
    loop {
        tokio::select! {
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(msg)) => {
                        if let Message::Text(text) = msg {
                            let response: Option<ClientMessageResponse> = match serde_json::from_str::<ClientMessage>(&text) {
                                Ok(msg) => {
                                    Some(ClientMessageResponse {
                                        id: msg.id,
                                        response: process_client_command(&state, &mut auth_user_id, &tx, msg.command).await,
                                    })
                                },
                                Err(e) => {
                                    eprintln!("parse error: {e}");
                                    None
                                }
                            };

                            if let Some(response) = response {
                                let json = serde_json::to_string(&response).unwrap();
                                socket.send(Message::Text(json.into())).await;
                            }                                                
                        }
                    },                    
                    Some(Err(e)) => {
                        eprintln!("websocket error: {e}");
                        break;
                    }
                }
            }
            game_event = rx.recv() => {
                match game_event {
                    Some(event) => {
                        let json = serde_json::to_string(&GameEventWire::from(&event)).unwrap();
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
        }
    }
}