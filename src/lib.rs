mod db;
use db::*;

mod songs;
use songs::*;

mod user;
use user::*;

use crate::user::Playlist;
use serde_json::json;
use async_once::AsyncOnce;
use dotenv;
use futures_util::{FutureExt, StreamExt};
use lazy_static::lazy_static;
use log::{debug, error, info, warn};
use std::collections::{BTreeMap, VecDeque};
use std::convert::Infallible;
use std::env;
use std::sync::Arc;
use std::{collections::HashMap, time::Duration};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_stream::wrappers::UnboundedReceiverStream;
use uuid::Uuid;
use warp::{
    ws::{Message, WebSocket},
    Filter, Rejection, Reply,
};

/*
 * If the variable we want to check is not set, then we'll abort
 * This is for required variables
 */
#[macro_export]
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
    // TODO better error handling here
    static ref DB: AsyncOnce<Database> = AsyncOnce::new(async { Database::new().await.unwrap() });

    // TODO swap out env vars for params
    static ref SONG_MANAGER: RwLock<SongManager> = RwLock::new(SongManager::new(None, None, None));
    static ref INSTANCE_KEY: String = env_fetch!("INSTANCE_KEY");
    static ref ADMIN_KEY: String = env::var("ADMIN_KEY").unwrap_or_default();
    pub static ref CACHE_DIR: String = env_fetch!("CACHE_DIR");

    // create list of username hashes that are blocked
    static ref BLOCKED_LIST: Arc<std::sync::RwLock<BlockedList>> = Arc::new(std::sync::RwLock::new(BlockedList::default()));

    // create the ratelimiter with the blocked list inside it, this is done seperately to allow
    // only loading the blocked list and not the current list as well
    static ref RATE_LIMIT: Arc<std::sync::Mutex<RateLimiter<'static>>> = Arc::new(std::sync::Mutex::new(RateLimiter::new(&BLOCKED_LIST)));

    // max request allowed per a second per an address
    static ref MAX_RATELIMIT: usize = env_num_or_default!("RATE_MAX_COUNT", 50) as usize;
}

const DEFAULT_PORT: u16 = 8080;

// time in seconds between downloading from queue
const DEFAULT_QUEUE_COOLDOWN: u32 = 10;

// a list of all the client's ips that connected are stored, this is the max length for that before
// the list does not get appended to
const MAX_CLIENT_RATE_CACHE: usize = 200;

const RATE_BAN_IN_SECONDS: usize = 60;

const RATE_BLACKLIST_CYCLE_MS: u32 = 10000;

/*
 * These are predefined commands that are valid to send client to client
 *
 * Similar to features in other platforms that allow you to control different player instances from
 * different devices, this lets you send commands to any client under the same username as yours
 *
 * With the way the database is setup you can only create unique usernames so this *shouldn't* be a
 * security issue
 */
static CLIENT_COMMANDS: [&'static str; 6] =
    ["PLAY", "PAUSE", "SKIP", "VOL_UP", "VOL_DOWN", "VOL_SET"];

/*
 * While the websocket clients are conneted we store an object so we can send to them
 * auth is used, as the name implies, for authentication
 * If a user does not authenticate on the first message over the websocket we remove them
 * immediately after logging their ip(TODO implement rate limiting) to prevent answering
 * unnecessary request
 *
 * The username_hash is used to communicate with other instances of itself
 */
pub(crate) struct WsClient {
    pub sender: Option<mpsc::UnboundedSender<std::result::Result<Message, warp::Error>>>,
    pub auth: bool,
    pub admin: bool,
    pub username_hash: u64,
}

// We store the websocket clients in this hashmap, the string being a uuid
pub(crate) type Clients = Arc<Mutex<HashMap<String, WsClient>>>;
type Result<T> = std::result::Result<T, Rejection>;

async fn ws_handler(ws: warp::ws::Ws, clients: Clients) -> Result<impl Reply> {
    Ok(ws.on_upgrade(move |socket| client_connection(socket, clients)))
}

#[macro_export]
macro_rules! acquire_db {
    ($val:expr) => {
        &$val.get().await
    };
}

async fn client_msg(client_id: &str, msg: &Message, clients: &Clients) {
    let msg = match msg.to_str() {
        Ok(v) => v,
        Err(_) => return,
    };
    let mut locked = clients.lock().await;
    match locked.get_mut(client_id) {
        Some(v) => {
            // potentially should switch this to it's own function to improve readability, also deal
            // with the username not supporting spaces in it somehow
            if !v.auth {
                let args = msg.split(" ").collect::<Vec<&str>>();
                match args[0] {
                    "AUTH" => {
                        if args.len() != 3
                            && !(args.len() == 4 && args[3] == &ADMIN_KEY.to_string())
                        {
                            // TODO CUSTOM ERROR
                            warn!("invalid args");
                            return;
                        }
                        match acquire_db!(DB)
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
                                    let _ = acquire_db!(DB)
                                        .update_login_timestamp(v.username_hash)
                                        .await;
                                    if args.len() == 4 && args[3] == &ADMIN_KEY.to_string() {
                                        if let Ok(admin) =
                                            acquire_db!(DB).is_admin(v.username_hash).await
                                        {
                                            // there are definitely better ways to do this
                                            if admin {
                                                v.admin = true;
                                            }
                                        }
                                    }
                                    // TODO proper error handling here
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
                        match acquire_db!(DB).new_user(args[1], args[2]).await {
                            Ok(_) => info!("inserted user"),
                            Err(_) => warn!("failed to insert user"),
                        };
                        // TODO CUSTOM SUCCESS
                    }
                    _ => return,
                };
                return;
            }
            handle_response(msg, &v, &clients, &client_id).await;
        }
        None => {}
    }
}

/*
 * Handle new client connection, we assign the new client a uuid and fill out some fields for it
 *
 * Fields:
 * - sender         = used to access the client and send to it
 * - auth           = determines if the client is authenticated
 * - username_hash  = hash of username to identify the client
 *
 * then we add to the hashmap of clients and wait for messages, when the client closes we can
 * remove them from the hashmap
 */
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
    // uuid v4 is 36 characters long
    let mut mapped_uuid = String::with_capacity(36);
    // I read in a book on rust clone from is faster, though I'm not really sure if this is true
    mapped_uuid.clone_from(&uuid);
    let new_client = WsClient {
        sender: Some(client_sender),
        auth: false,
        admin: false,
        username_hash: 0,
    };
    clients.lock().await.insert(mapped_uuid, new_client);
    info!("s");
    while let Some(result) = rx.next().await {
        let msg = match result {
            Ok(msg) => msg,
            Err(e) => {
                warn!("error receiving message for id {}: {e}", &uuid);
                break;
            }
        };
        client_msg(&uuid, &msg, &clients).await;
    }
    clients.lock().await.remove(&uuid);
    info!("{} disconnected", uuid);
}

/*
 * Send text message to clients
 */
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

macro_rules! disconnect {
    ($val:expr, $uuid:expr) => {
        let connection = &mut $val.lock().await;
        let client = match connection.get($uuid) {
            Some(v) => v,
            None => return,
        };
        // Close the connection to the websocket
        if let Some(sender) = &client.sender {
            let _ = sender.closed();
        }
        connection.remove($uuid);
        info!("{} disconnected", $uuid);
    };
}
// TODO api calls
// follow
// unfollow
// update userdata - need to check for display name updates to update followers list for other
// people if needed

/*
 * Respond to messages sent from a client, if one of the prestored "client commands" is executed
 * then we can just echo the message, this *should* be secure since you can only send to usernames
 * that you are logged in under, but I could be very wrong
 */
async fn handle_response<'a>(
    msg: &str,
    ws_client: &WsClient,
    clients: &Clients,
    client_uuid: &str,
) {
    debug!("{msg}");
    if BLOCKED_LIST
        .read()
        .unwrap()
        .list
        .contains_key(&ws_client.username_hash)
    {
        return;
    }
    RATE_LIMIT.lock().unwrap().add(ws_client.username_hash);
    if let Some(v) = msg.find(" ") {
        let command = &msg[..v];
        let message = &msg[v..].trim_start();
        if CLIENT_COMMANDS.contains(&command) {
            send_to_clients!(clients, ws_client, msg);
            return;
        }
        let args: Vec<&str> = message.split(" ").collect();
        let response = match command {
            "PING" => Some(String::from("PONG")),
            "QUEUE" => {
                // Send new song to download queue
                let mut locked = SONG_MANAGER.write().await;
                Some(String::from(match locked.request(message.to_string()) {
                    Ok(_) => "AddedSong",
                    Err(_) => "InvalidRequest",
                }))
            }
            "QUEUE_LIST" => {
                let locked = SONG_MANAGER.read().await;
                Some(locked.list_queue())
            }
            "FIND_SONG" => match args.len() {
                3 => {
                    match acquire_db!(DB)
                        .find_song_from_details(args[0], args[1], args[2])
                        .await
                    {
                        Ok(v) => Some(v.to_string()),
                        Err(_) => None,
                    }
                }
                _ => None,
            },
            "REMOVE_SONG" => match args.len() {
                4 => {
                    match acquire_db!(DB)
                        .remove_song(ws_client.username_hash, args[0], args[1], args[2], args[3])
                        .await
                    {
                        Ok(_) => Some(String::from("OK")),
                        Err(_) => Some(String::from("CouldNotBeFound")),
                    }
                }
                _ => None,
            },
            "SONG_LIST_SHORT" => match acquire_db!(DB).get_song_list().await {
                Ok(v) => Some(v),
                Err(_) => None,
            },
            "ADD_SONG" => match args.len() {
                4 => {
                    match acquire_db!(DB)
                        .append_song(ws_client.username_hash, args[0], args[1], args[2], args[3])
                        .await
                    {
                        Ok(_) => Some(String::from("OK")),
                        Err(_) => Some(String::from("CouldNotFindSong")),
                    }
                }
                _ => None,
            },
            "ADD_SONG_HASH" => match args.len() {
                3 => match args[1].parse::<u64>() {
                    Ok(v) => {
                        match acquire_db!(DB)
                            .append_song_from_hash(ws_client.username_hash, args[0], v)
                            .await
                        {
                            Ok(()) => Some(String::from("OK")),
                            Err(_) => Some(String::from("InvalidHash")),
                        }
                    }
                    Err(_) => Some(String::from("ExpectedHash")),
                },
                _ => None,
            },
            "MAKE_PLAYLIST" => match args.len() {
                2 => {
                    match acquire_db!(DB)
                        .create_playlist(ws_client.username_hash, args[0], args[1])
                        .await
                    {
                        Ok(()) => Some(String::from("OK")),
                        Err(_) => Some(String::from("InvalidHash")),
                    }
                }
                _ => None,
            },
            "EDIT_PLAYLIST" => match args.len() {
                3.. => {
                    let jsonify: Playlist =
                        match serde_json::from_str(&message[args[0].len() - 1..]) {
                            Ok(v) => v,
                            Err(_) => return,
                        };

                    match acquire_db!(DB)
                        .update_playlist(ws_client.username_hash, args[0], jsonify)
                        .await
                    {
                        Ok(_) => Some(String::from("OK")),
                        Err(_) => None,
                    }
                }
                _ => None,
            },
            "REMOVE_PLAYLIST" => {
                match args.len() {
                    1 => {
                        // apply delim change here too, create macro or function for it
                        match acquire_db!(DB)
                            .delete_playlist(ws_client.username_hash, args[0])
                            .await
                        {
                            Ok(()) => Some(String::from("OK")),
                            Err(_) => Some(String::from("InvalidHash")),
                        }
                    }
                    _ => None,
                }
            }
            "SET_PLAYLIST_IMAGE" => match args.len() {
                2 => {
                    match acquire_db!(DB)
                        .set_playlist_image(ws_client.username_hash, args[0], args[1])
                        .await
                    {
                        Ok(()) => Some(String::from("OK")),
                        Err(_) => Some(String::from("InvalidHash")),
                    }
                }
                _ => None,
            },
            "SET_PLAYLIST_DESCRIPTION" => {
                match args.len() {
                    2 => {
                        // TODO
                        // parse with % delim instead of spaces so there can be a name in the
                        // description
                        //
                        // also actually do this for rename playlist and make playlist so there can
                        // be spaces in the names
                        match acquire_db!(DB)
                            .set_playlist_description(ws_client.username_hash, args[0], args[1])
                            .await
                        {
                            Ok(()) => Some(String::from("OK")),
                            Err(_) => Some(String::from("InvalidDescription")),
                        }
                    }
                    _ => None,
                }
            }
            "RENAME_PLAYLIST" => match args.len() {
                2 => {
                    match acquire_db!(DB)
                        .rename_playlist(ws_client.username_hash, args[0], args[1])
                        .await
                    {
                        Ok(()) => Some(String::from("OK")),
                        Err(_) => Some(String::from("InvalidHash")),
                    }
                }
                _ => None,
            },
            "REQUEST_USER_DATA" => {
                match acquire_db!(DB).get_user_data(ws_client.username_hash).await {
                    Ok(v) => {
                        let data = json!(&v).to_string();
                        info!("Sending userdata: {data}");
                        Some(data)
                    }
                    Err(_) => None,
                }
            }
            "UPDATE_USERDATA" => match args.len() {
                3.. => Some(String::from("OK")),
                _ => None,
            },
            "CLOSE" => {
                disconnect!(clients, client_uuid);
                None
            }
            _ => None,
        };
        if let Some(data_out) = response {
            if let Some(v) = &ws_client.sender {
                let _ = v.send(Ok(Message::text(data_out)));
            }
        }
    }
}

fn with_clients(clients: Clients) -> impl Filter<Extract = (Clients,), Error = Infallible> + Clone {
    warp::any().map(move || clients.clone())
}

/*
 * Start logger, check env arguments, create a new client map
 *
 * The route "seanify" (example: 127.0.0.1:3030/seanify) is the route that is connected to access
 * the main service
 *
 * The route with the value of INSTANCE_KEY is used to store music download, it points to the cache
 * directory
 *
 * If I want to download a song, I'll send the server a SEARCH request and it will return a hash of
 * the song (this can also be computed client side), visiting the websocket at this route will
 * reveal the song file
 *
 * For example, if a song has a hash of 12 and my instance key is songs then it can be downloaded
 * from 127.0.0.1:3030/songs/12
 */

//TODO add arg handling
pub async fn run<S: AsRef<str>>(_args: &[S]) -> anyhow::Result<()> {
    pretty_env_logger::init();
    check_env_args().unwrap();

    // go through queue every x amount of seconds to attempt to download the first song
    // 5 second default
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(
                env_num_or_default!("QUEUE_COOLDOWN", DEFAULT_QUEUE_COOLDOWN).into(),
            ))
            .await;
            let mut locked = SONG_MANAGER.write().await;
            let _ = locked.cycle_queue().await;
        }
    });

    tokio::spawn(async move {
        // ip blacklist cycle
        loop {
            tokio::time::sleep(Duration::from_millis(env_num_or_default!(
                "RATE_BLACKLIST_CYCLE_MS",
                RATE_BLACKLIST_CYCLE_MS
            ) as u64))
            .await;
            let mut locked = RATE_LIMIT.lock().unwrap();
            locked.cycle();
        }
    });

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let mut locked = BLOCKED_LIST.write().unwrap();
            for (_, v) in locked.list.iter_mut() {
                *v -= 1;
            }
            locked.list.retain(|_, v| v > &mut 0);
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

    // TODO require check for cdndir
    let cdn = warp::path(format!("{}-cdn", INSTANCE_KEY.to_string()))
        .and(warp::fs::dir(env_fetch!("CDN_DIR")))
        .with(warp::compression::gzip());

    // unfortunate conversions has to be done here, might be worth fixing in the future
    warp::serve(routes.or(music).or(cdn))
        .run((
            [127, 0, 0, 1],
            env_num_or_default!("PORT", DEFAULT_PORT as u32) as u16,
        ))
        .await;

    Ok(())
}

// Keep list of blocked ips, we store them seperately so it's quicker to access since we don't need
// to load the total list of ips
#[derive(Default)]
struct BlockedList {
    list: BTreeMap<u64, usize>,
}

/*
 * Keep a list of ips that have connected and cycle through them every set amount of ms, if there
 * are too many instances of the same ip in the list at the same time we add them to the blocked
 * list
 *
 * Since the blockedlist needs to be read everytime there is a new connection we only store a
 * reference to it in this struct so we can avoid locking both the ratelimiter and blockedlist at
 * the same time
 *
 * Since the blocked list is going to be read a lot more often we keep it in an RwLock instead of a
 * mutex
 */
struct RateLimiter<'a> {
    username_list: VecDeque<u64>,
    blocked_list: &'a Arc<std::sync::RwLock<BlockedList>>,
}

impl<'a> RateLimiter<'a> {
    pub fn new(blocked_list: &'a Arc<std::sync::RwLock<BlockedList>>) -> Self {
        Self {
            username_list: VecDeque::with_capacity(1),
            blocked_list,
        }
    }

    pub fn add(&mut self, username: u64) {
        if self.username_list.len() < MAX_CLIENT_RATE_CACHE {
            self.username_list.push_back(username);
        }
    }

    pub fn cycle(&mut self) {
        let _ = self.username_list.pop_front();
        self.check_if_limited();
    }

    fn check_if_limited(&mut self) {
        let mut usernames = BTreeMap::new();
        for username in self.username_list.iter() {
            let count = usernames.entry(username).or_insert(0);
            *count += 1;
        }
        for username in usernames.iter() {
            if let Some(v) = usernames.get(username.0) {
                if v > &*MAX_RATELIMIT {
                    let mut locked = self.blocked_list.write().unwrap();
                    // "**" lmao wtf
                    locked.list.insert(**username.0, RATE_BAN_IN_SECONDS);
                }
            }
        }
    }
}

macro_rules! check_or_warn_env {
    ($val:expr) => {
        match env::var($val) {
            Ok(v) => info!("{}: {v}", $val),
            Err(_) => warn!("It is recommended to set {} in env", $val),
        }
    };
}

/*
 * Check the env variables for certain arguments, these are non essential and only emit a warning
 * at startup if not present
 */
fn check_env_args() -> anyhow::Result<()> {
    dotenv::dotenv()?;

    let uri = env_fetch!("DATABASE_URL");

    info!("Database: {uri}");

    let vars = [
        "MAX_CONNECTIONS",
        "MAX_TIMEOUT",
        "MAX_CACHE_SIZE_MB",
        "QUEUE_COOLDOWN",
        "PORT",
        "ADMIN_KEY",
        "RATE_BAN_IN_SECONDS",
        "RATE_MAX_COUNT",
        "RATE_BLACKLIST_CYCLE_MS"
    ];

    vars.iter().for_each(|x| check_or_warn_env!(x));
    Ok(())
}
