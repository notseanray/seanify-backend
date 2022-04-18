mod db;
use db::*;

mod songs;
use songs::*;

use async_once::AsyncOnce;
use dotenv;
use futures_util::{FutureExt, StreamExt};
use lazy_static::lazy_static;
use log::{error, info, warn};
use std::convert::Infallible;
use std::env;
use std::sync::Arc;
use std::{collections::HashMap, time::Duration};
use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::UnboundedReceiverStream;
use uuid::Uuid;
use warp::{
    ws::{Message, WebSocket},
    Filter, Rejection, Reply,
};

macro_rules! env_fetch {
    ($val:expr) => {
        match env::var($val) {
            Ok(v) => v,
            Err(_) => {
                error!("Please set {} in env", $val);
                std::process::exit(1);
            }
        }
    };
}

lazy_static! {
    static ref DB: AsyncOnce<Database> = AsyncOnce::new(async { Database::new().await.unwrap() });
    static ref SONG_MANAGER: Mutex<SongManager> = Mutex::new(SongManager::new(None, None, None));
    static ref INSTANCE_KEY: String = env_fetch!("INSTANCE_KEY");
    pub static ref CACHE_DIR: String = env_fetch!("CACHE_DIR");
}

static CLIENT_COMMANDS: [&'static str; 6] =
    ["PLAY", "PAUSE", "SKIP", "VOL_UP", "VOL_DOWN", "VOL_SET"];

pub(crate) struct WsClient {
    pub client_id: String,
    pub sender: Option<mpsc::UnboundedSender<std::result::Result<Message, warp::Error>>>,
    pub auth: bool,
    pub connections_in_last_minute: u16,
    pub username_hash: u64,
}

pub(crate) type Clients = Arc<Mutex<HashMap<String, WsClient>>>;
pub type Result<T> = std::result::Result<T, Rejection>;

async fn ws_handler(ws: warp::ws::Ws, clients: Clients) -> Result<impl Reply> {
    Ok(ws.on_upgrade(move |socket| client_connection(socket, clients)))
}

async fn client_msg(client_id: &str, msg: &Message, clients: &Clients) {
    let msg = match msg.to_str() {
        Ok(v) => v,
        Err(_) => return,
    };
    let mut locked = clients.lock().await;
    match locked.get_mut(client_id) {
        Some(v) => {
            if !v.auth {
                let args = msg.split(" ").collect::<Vec<&str>>();
                match args[0] {
                    "AUTH" => {
                        if args.len() != 3 {
                            // TODO CUSTOM ERROR
                            warn!("invalid args");
                            return;
                        }
                        match DB
                            .get()
                            .await
                            .check_if_user_exists_in_auth(args[1], args[2])
                            .await
                        {
                            Ok(r) => {
                                if r.0 {
                                    v.username_hash = match r.1 {
                                        Some(v) => v,
                                        None => {
                                            //TODO custom error
                                            warn!("unhashable username");
                                            return;
                                        }
                                    };
                                    v.auth = true;
                                    info!("authenticated user")
                                } else {
                                    locked.remove(client_id);
                                    info!("auth failed, removed client");
                                }
                            }
                            Err(e) => {
                                info!("removed client due to {e}");
                                locked.remove(client_id);
                                // TODO RESPOND WITH AUTH FAIL
                            }
                        };
                    }
                    "SIGN" => {
                        if args.len() != 4 {
                            // TODO CUSTOM ERROR
                            warn!("invalid args");
                            return;
                        }
                        if args[3] != INSTANCE_KEY.as_str() {
                            // TODO CUSTOM ERROR
                            warn!("invalid instance key");
                            return;
                        }
                        if let Ok(true) = DB
                            .get()
                            .await
                            .check_if_username_exists_in_auth(args[1])
                            .await
                        {
                            warn!("username already exist");
                            return;
                        }
                        // TODO CUSTOM RESPONSE
                        match DB.get().await.new_user(args[1], args[2]).await {
                            Ok(_) => info!("inserted user"),
                            Err(_) => warn!("failed to insert user"),
                        };
                        // TODO CUSTOM SUCCESS
                    }
                    _ => return,
                };
                return;
            }
            if let Some(response) = handle_response(msg, &v, &clients).await {
                if let Some(sender) = &v.sender {
                    let _ = sender.send(Ok(Message::text(response)));
                }
            }
        }
        None => {}
    }
}

async fn client_connection(ws: WebSocket, clients: Clients) {
    info!("establishing new client connection...");
    let (tx, mut rx) = ws.split();
    let (client_sender, client_rcv) = mpsc::unbounded_channel();
    let client_rcv = UnboundedReceiverStream::new(client_rcv);
    tokio::task::spawn(client_rcv.forward(tx).map(|result| {
        if let Err(e) = result {
            warn!("error sending websocket msg: {e}");
        }
    }));
    let uuid = Uuid::new_v4().to_simple().to_string();
    let new_client = WsClient {
        client_id: uuid.clone(),
        sender: Some(client_sender),
        auth: false,
        connections_in_last_minute: 0,
        username_hash: 0,
    };
    clients.lock().await.insert(uuid.clone(), new_client);
    while let Some(result) = rx.next().await {
        let msg = match result {
            Ok(msg) => msg,
            Err(e) => {
                warn!("error receiving message for id {uuid}): {e}");
                break;
            }
        };
        client_msg(&uuid, &msg, &clients).await;
    }
    clients.lock().await.remove(&uuid);
    info!("{} disconnected", uuid);
}

macro_rules! send_to_clients {
    ($clients:expr, $ws_client:expr, $msg:expr) => {
        for (_, value) in $clients.lock().await.iter() {
            if &value.username_hash != &$ws_client.username_hash {
                continue;
            }
            if let Some(sender) = &value.sender {
                let _ = sender.send(Ok(Message::text($msg)));
            }
        }
    };
}

async fn handle_response<'a>(
    msg: &str,
    ws_client: &WsClient,
    clients: &Clients,
) -> Option<&'a str> {
    info!("{msg}");
    if let Some(v) = msg.find(" ") {
        let command = &msg[..v];
        let message = &msg[v..].trim_start();
        if CLIENT_COMMANDS.contains(&command) {
            send_to_clients!(clients, ws_client, msg);
        }
        return match command {
            "PING" => Some("PONG"),
            "QUEUE" => {
                let mut locked = SONG_MANAGER.lock().await;
                locked.request(message.to_string());
                None
            },
            "SEARCH" => {
                None
            },
            _ => None,
        };
    }
    None
}

fn with_clients(clients: Clients) -> impl Filter<Extract = (Clients,), Error = Infallible> + Clone {
    warp::any().map(move || clients.clone())
}

pub async fn run<S: AsRef<str>>(_args: &[S]) -> anyhow::Result<()> {
    pretty_env_logger::init();
    check_env_args().unwrap();

    // go through queue every 5 seconds to attempt to download the first song
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            let mut locked = SONG_MANAGER.lock().await;
            let _ = locked.cycle_queue().await;
        }
    });

    let clients: Clients = Arc::new(Mutex::new(HashMap::new()));
    let ws_route = warp::path("seanify")
        .and(warp::ws())
        .and(with_clients(clients.clone()))
        .and_then(ws_handler);
    let routes = ws_route.with(warp::cors().allow_any_origin());

    let music = warp::path(INSTANCE_KEY.to_string())
        .and(warp::fs::dir(CACHE_DIR.to_string()))
        .with(warp::compression::gzip());

    let routes = routes.or(music);
    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;

    Ok(())
}

macro_rules! check_or_warn_env {
    ($val:expr) => {
        match env::var($val) {
            Ok(v) => info!("{}: {v}", $val),
            Err(_) => warn!("It is recommended to set {} in env", $val),
        }
    };
}

fn check_env_args() -> anyhow::Result<()> {
    dotenv::dotenv()?;

    let uri = env_fetch!("DATABASE_URL");

    info!("Database: {uri}");

    let vars = ["MAX_CONNECTIONS", "MAX_TIMEOUT", "MAX_CACHE_SIZE_MB"];
    vars.iter().for_each(|x| check_or_warn_env!(x));
    Ok(())
}
