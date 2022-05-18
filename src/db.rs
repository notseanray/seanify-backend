use crate::pictures::{default_playlist_image, save_playlist_image};
use crate::user::Playlist;
use crate::{UserData, UserDataBigD};
use anyhow::anyhow;
use log::{error, info, LevelFilter};
use num_traits::ToPrimitive;
use seahash::hash;
use serde::Serialize;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, Postgres};
use sqlx::ConnectOptions;
use sqlx::Pool;
use std::env::{self, var};
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::fs::remove_file;

use crate::env_fetch;
use crate::songs::Song;

const DEFAULT_MAX_CONNECTIONS: u32 = 3;
const DEFAULT_MAX_TIMEOUT: u32 = 2;

pub(crate) type BigD = sqlx::types::BigDecimal;

// Only used for authentication and signup
struct UserAuth {
    pub username: Option<BigD>,
    pub username_u64: u64,
    pub password: Option<BigD>,
    pub last_login: Option<BigD>,
}

// A struct must be used for query_as! macro (from what I can tell), so to read if the user exists
// from the database output we must have a struct
struct Exists {
    pub exists: Option<bool>,
}

// Used to fetch hash directly from database, postgres does not have a u64 datatype so NUMERIC must
// be used, which needs BigDecimal
struct UserHash {
    pub username: BigD,
}

// Used to fetch the user displayname from the hash
struct DisplayName {
    pub display_name: Option<String>,
}

// return the first x amount of characters in a string (and should be unicode safe)
macro_rules! truncate {
    ($val:expr, $len:expr) => {
        $val.map(|v| match v.len() > $len {
            true => {
                let mut new_input = String::with_capacity($len);
                // use chars() instead of index to prevent panics
                let shortened = &v.chars().collect::<Vec<char>>()[..$len];
                shortened.iter().for_each(|x| new_input.push(*x));
                new_input
            }
            false => v,
        })
    };
}

// return seconds since unix epoch as a BigDecimal
macro_rules! time {
    () => {
        BigD::from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        )
    };
}

// Hash the username and password then return, update the last login time
impl UserAuth {
    pub fn new(username: &str, password: &str) -> Self {
        let name = hash(username.as_bytes());
        Self {
            username: Some(name.into()),
            username_u64: name,
            password: Some(hash(password.as_bytes()).into()),
            last_login: Some(time!()),
        }
    }
}

// DatabasePool
pub(crate) struct Database {
    pub database: Pool<Postgres>,
}

// When we look up just the hash of a song we have to use a struct to return it's id/hash
struct SongLookupResult {
    id: BigD,
}

// result from fetching a song from the database, when a client wants to look up a song it recieves
// this data
pub(crate) struct SongTitleResult {
    pub id: BigD,
    pub title: String,
    pub uploader: Option<String>,
    pub thumbnail: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub artist: Option<String>,
    pub creator: Option<String>,
    pub upload_date: Option<String>,
    pub downloaded: bool,
}

#[derive(Serialize)]
pub(crate) struct SongTitleResultOut {
    pub id: String,
    pub title: String,
    pub uploader: Option<String>,
    pub thumbnail: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub artist: Option<String>,
    pub creator: Option<String>,
    pub upload_date: Option<String>,
    pub downloaded: bool,
}

impl From<SongTitleResult> for SongTitleResultOut {
    fn from(s: SongTitleResult) -> Self {
        Self {
            id: s.id.to_u64().unwrap_or_default().to_string(),
            title: s.title,
            uploader: s.uploader,
            thumbnail: s.thumbnail,
            album: s.album,
            album_artist: s.album_artist,
            artist: s.artist,
            creator: s.creator,
            upload_date: s.upload_date,
            downloaded: s.downloaded
        } 
    }
}

struct SongDetails {
    id: BigD,
    title: String,
}

macro_rules! to_big_d {
    ($val:expr) => {
        match $val {
            Some(v) => BigD::from(v),
            None => return Ok(()), // TODO proper error handling
        }
    };
}

#[macro_export]
macro_rules! env_num_or_default {
    ($val:expr, $default:expr) => {
        match std::env::var($val)
            .unwrap_or(String::from(""))
            .parse::<u32>()
        {
            Ok(v) => v,
            Err(e) => {
                error!(
                    "{} is invalid due to: {e}, using default of {}",
                    $val, $default
                );
                $default
            }
        }
    };
}

impl Database {
    pub async fn new() -> anyhow::Result<Self> {
        let uri = var("DATABASE_URL").unwrap();
        Ok(Self {
            database: Self::try_connect(&uri).await,
        })
    }

    pub async fn try_connect(uri: &str) -> Pool<Postgres> {
        for i in 1..=5 {
            match Self::connect(uri).await {
                Ok(v) => return v,
                Err(e) => info!("Failed to connect to {uri} due to {e}, retrying [{i}/5]"),
            };
            sleep(Duration::from_millis(300));
        }
        error!("could not aquire database after 5 attempts, quiting");
        std::process::exit(1);
    }

    async fn connect(uri: &str) -> anyhow::Result<Pool<Postgres>> {
        let mut connect_opts = PgConnectOptions::new();
        connect_opts.log_statements(LevelFilter::Debug);

        let timeout = env_num_or_default!("MAX_TIMEOUT", DEFAULT_MAX_TIMEOUT);
        let max_connections = env_num_or_default!("MAX_CONNECTIONS", DEFAULT_MAX_CONNECTIONS);

        let my_pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .connect_timeout(Duration::from_secs(timeout.into()))
            .connect(uri)
            .await?;

        Ok(my_pool)
    }

    pub async fn update_downloaded(&self, hash: u64) -> anyhow::Result<()> {
        sqlx::query!(
            "
UPDATE 
    songs 
SET 
    downloaded_timestamp = $3,
    downloaded = $2 
WHERE 
    id = $1;
            ",
            BigD::from(hash),
            true,
            time!()
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;

        Ok(())
    }

    // Might have to fix this, I am not good enough at SQL to actually see if this works as
    // intended since the songs of the same type have the exact same id
    //
    // remove old copies of a song if they are redownloaded, (based off id only)
    pub async fn remove_duplicate_songs(&self) -> anyhow::Result<()> {
        sqlx::query!(
            "
DELETE FROM 
    songs s 
    USING songs b 
WHERE 
    s.downloaded_timestamp < b.downloaded_timestamp
    AND s.id = b.id;
            "
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;

        Ok(())
    }

    pub async fn sync_library(&self, timestamp: u64) -> anyhow::Result<String> {
        let data: Vec<SongTitleResult> = sqlx::query_as!(
            SongTitleResult,
            "
SELECT 
    id,
    title, 
    uploader, 
    thumbnail, 
    album, 
    album_artist, 
    artist, 
    creator, 
    upload_date, 
    downloaded 
FROM 
    songs
WHERE
    downloaded_timestamp >= $1
            ",
            BigD::from(timestamp)
        )
        .fetch_all(&mut self.database.acquire().await?)
        .await?;
        let mut songs: Vec<SongTitleResultOut> = Vec::with_capacity(data.len());
        for song in data {
            songs.push(song.into());
        }

        match serde_json::to_string(&songs) {
            Ok(v) => Ok(v),
            Err(_) => Err(anyhow!("FailedToSync")),
        }
    }

    pub async fn insert_song(&self, song: Song) -> anyhow::Result<()> {
        sqlx::query!(
            "
 INSERT INTO 
    songs(
        id, 
        title, 
        upload_date, 
        uploader, 
        url, 
        genre,
        thumbnail, 
        album, 
        album_artist, 
        artist, 
        creator, 
        filesize, 
        downloaded_timestamp, 
        downloaded
    )
 VALUES($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14);
             ",
            to_big_d!(song.id),
            song.title,
            song.upload_date,
            song.uploader,
            song.url,
            song.genre,
            song.thumbnail,
            song.album,
            song.album_artist,
            song.artist,
            song.creator,
            song.filesize,
            BigD::from(0),
            false
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;

        Ok(())
    }

    pub async fn userhash_from_username(&self, display_name: &str) -> anyhow::Result<BigD> {
        let hash = sqlx::query_as!(
            UserHash,
            "
SELECT 
    username 
FROM 
    auth 
WHERE 
    (
        SELECT 
            (userdata).display_name 
        FROM 
            auth
    ) = $1;
            ",
            display_name.to_owned(),
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;
        match hash {
            Some(v) => Ok(v.username),
            None => Err(anyhow!("no user of that name")),
        }
    }

    pub async fn unfollow_user(&self, userhash: u64, name_to_unfollow: &str) -> anyhow::Result<()> {
        let hash = self.userhash_from_username(name_to_unfollow).await?;
        sqlx::query!(
            "
UPDATE 
    auth 
SET
    userdata.following = array_remove(
        (
            SELECT 
                (userdata).following 
            FROM 
                auth
        ), 
        $1
    )
WHERE 
    username = $2; 
            ",
            hash,
            BigD::from(userhash)
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;

        sqlx::query!(
            "
UPDATE 
    auth 
SET
    userdata.followers = array_remove(
        (
            SELECT 
                (userdata).followers 
            FROM 
                auth
        ), 
        $1)
WHERE 
    username = $2; 
            ",
            BigD::from(userhash),
            hash
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;
        Ok(())
    }

    pub async fn follow_user(&self, userhash: u64, name_to_follow: &str) -> anyhow::Result<()> {
        let hash = self.userhash_from_username(name_to_follow).await?;
        sqlx::query!(
            "
UPDATE 
    auth 
SET
    userdata.following = array_append(
        (
            SELECT 
                (userdata).following 
            FROM 
                auth
        ), 
        $1)
WHERE 
    username = $2; 
            ",
            hash,
            BigD::from(userhash)
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;

        sqlx::query!(
            "
UPDATE 
    auth 
SET
    userdata.followers = array_append(
        (
            SELECT 
                (userdata).followers 
            FROM 
                auth
        ), 
        $1)
WHERE 
    username = $2; 
            ",
            self.userhash_from_username(name_to_follow).await?,
            hash
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;
        Ok(())
    }

    pub async fn new_user(&self, username: &str, password: &str) -> anyhow::Result<()> {
        let user = UserAuth::new(username, password);
        sqlx::query!(
            "
INSERT INTO 
    auth(
        username, 
        password, 
        admin, 
        last_login
    )
VALUES($1, $2, false, $3);
            ",
            user.username,
            user.password,
            user.last_login
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;

        let data = UserData::default();
        sqlx::query!(
            "
UPDATE 
    auth 
SET
    userdata.public_profile = $1, 
    userdata.display_name = $2, 
    userdata.share_status = $3, 
    userdata.now_playing = $4, 
    userdata.public_status = $5, 
    userdata.recent_plays = $6, 
    userdata.followers = $7, 
    userdata.following = $8
WHERE 
    username = $9;
            ",
            data.public_profile,
            data.display_name,
            data.share_status,
            data.now_playing,
            data.public_status,
            &data.recent_plays.unwrap_or_default()[..], // FUCK POSTGRES
            &data
                .followers
                .unwrap_or_default()
                .iter()
                .map(|x| BigD::from(*x))
                .collect::<Vec<BigD>>(),
            &data
                .following
                .unwrap_or_default()
                .iter()
                .map(|x| BigD::from(*x))
                .collect::<Vec<BigD>>(),
            user.username
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;

        Ok(())
    }

    async fn is_name_taken(&self, name: &str) -> anyhow::Result<bool> {
        let mut names = sqlx::query_as!(
            DisplayName,
            "
SELECT 
    (userdata).display_name 
FROM 
    auth 
WHERE 
    (userdata).display_name = $1;
            ",
            name
        )
        .fetch_all(&mut self.database.acquire().await?)
        .await?;
        names.retain(|x| x.display_name.is_some());
        Ok(!names.is_empty())
    }

    pub async fn set_userdata(&self, username: u64, new_data: UserData) -> anyhow::Result<()> {
        match &new_data.display_name {
            Some(v) => {
                if self.is_name_taken(v).await? {
                    return Err(anyhow!("DisplayNameAlreadyTaken"));
                }
            }
            None => {}
        };

        sqlx::query!(
            "
UPDATE 
    auth 
SET
    userdata.public_profile = $1, 
    userdata.display_name = $2, 
    userdata.share_status = $3, 
    userdata.now_playing = $4, 
    userdata.public_status = $5, 
    userdata.recent_plays = $6
WHERE 
    username = $7;
            ",
            new_data.public_profile,
            new_data.display_name,
            new_data.share_status,
            new_data.now_playing,
            new_data.public_status,
            &new_data.recent_plays.unwrap_or_default()[..], // FUCK POSTGRES
            BigD::from(username)
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;

        Ok(())
    }

    pub async fn find_song_from_details(
        &self,
        song_name: &str,
        song_author: &str,
        song_release: &str,
    ) -> anyhow::Result<BigD> {
        let result = sqlx::query_as!(
            SongLookupResult,
            "
SELECT 
    id 
FROM 
    songs
WHERE 
    title = $1 AND creator = $2 
    AND upload_date = $3;
            ",
            song_name,
            song_author, // refactor? this is misleading as it's the yt uploader NOT the
            // artist/author of song
            song_release
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;

        if let Some(v) = result {
            return Ok(v.id);
        }
        Err(anyhow!("no song exists"))
    }

    pub async fn remove_song(
        &self,
        username: u64,
        playlist_name: &str,
        song_name: &str,
        song_author: &str,
        song_release: &str,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            "
DELETE FROM 
    playlistdata
WHERE 
    username = $1 
    AND playlist_name = $2 
    AND song_hash = $3;
            ",
            BigD::from(username),
            playlist_name, // check if valid playlist
            self.find_song_from_details(song_name, song_author, song_release)
                .await
                .unwrap()
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;

        self.update_playlist_timestamp(username, playlist_name)
            .await?;

        Ok(())
    }

    async fn find_song_from_hash(&self, song_hash: u64) -> anyhow::Result<SongDetails> {
        let result = sqlx::query_as!(
            SongDetails,
            "
SELECT 
    id, title 
FROM 
    songs
WHERE 
    id = $1;
            ",
            BigD::from(song_hash)
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;

        match result {
            Some(v) => Ok(v),
            None => Err(anyhow!("Invalid song")),
        }
    }

    pub async fn update_playlist(
        &self,
        username: u64,
        playlist_name: &str,
        data: Playlist,
    ) -> anyhow::Result<()> {
        let mut data = data;

        data.name = truncate!(Some(data.name), 30).unwrap();

        data.description = truncate!(data.description, 200);

        sqlx::query!(
            "
UPDATE 
    playlist 
SET
    name = $1,
    description = $2,
    public_playlist = $3,
    last_update = $4
WHERE 
    username = $5 
    AND name = $6
            ",
            data.name,
            data.description,
            data.public_playlist,
            time!(),
            BigD::from(username),
            playlist_name
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;

        self.update_playlist_timestamp(username, &data.name).await?;

        Ok(())
    }

    pub async fn append_song_from_hash(
        &self,
        username: u64,
        playlist_name: &str,
        song_hash: u64,
    ) -> anyhow::Result<()> {
        let song = self.find_song_from_hash(song_hash).await?;
        sqlx::query!(
            "
INSERT INTO 
    playlistdata(
        username,
        playlist_name,
        song_hash,
        song_name,
        date_added
    )
VALUES($1, $2, $3, $4, $5);
            ",
            BigD::from(username),
            playlist_name, // check if valid playlist
            song.id,
            song.title,
            time!()
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;

        self.update_playlist_timestamp(username, playlist_name)
            .await?;

        Ok(())
    }

    pub async fn append_song(
        &self,
        username: u64,
        playlist_name: &str,
        song_name: &str,
        song_author: &str,
        song_release: &str,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            "
INSERT INTO 
    playlistdata(
        username,
        playlist_name,
        song_hash,
        song_name,
        date_added
    )
VALUES($1, $2, $3, $4, $5);
            ",
            BigD::from(username),
            playlist_name, // check if valid playlist
            BigD::from(
                self.find_song_from_details(song_name, song_author, song_release)
                    .await
                    .unwrap()
            ),
            song_name,
            time!()
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;

        self.update_playlist_timestamp(username, playlist_name)
            .await?;

        Ok(())
    }

    pub async fn update_playlist_timestamp(
        &self,
        username: u64,
        play_list_name: &str,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            "
UPDATE 
    playlist 
SET 
    last_update = $1
WHERE 
    username = $2 
    AND name = $3;
                ",
            time!(),
            BigD::from(username),
            play_list_name
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;

        Ok(())
    }

    async fn does_playlist_exists(&self, username: u64, name: &str) -> anyhow::Result<bool> {
        let result = sqlx::query_as!(
            Exists,
            "
SELECT EXISTS(
    SELECT 
        1 
    FROM 
        playlist 
    WHERE 
        username = $1 
        AND name = $2 
    LIMIT 1
);
            ",
            BigD::from(username),
            name
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;
        Ok(match result {
            Some(v) => v.exists.unwrap_or_default(),
            None => true,
        })
    }

    pub async fn create_playlist(
        &self,
        username: u64,
        name: &str,
        public_playlist: &str,
    ) -> anyhow::Result<()> {
        if let Ok(true) = self.does_playlist_exists(username, name).await {
            info!("playlist already exists");
            return Ok(());
        }

        let public_playlist = match public_playlist.to_lowercase().as_str() {
            "true" => true,
            "false" => false,
            _ => return Err(anyhow!("InvalidMessage")),
        };

        let timestamp = time!();

        sqlx::query!(
            "
INSERT INTO 
    playlist(
        username, 
        name, 
        creation_timestamp, 
        public_playlist, 
        last_update
    )
VALUES($1, $2, $3, $4, $5);
            ",
            BigD::from(username),
            name,
            timestamp,
            public_playlist,
            timestamp
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;

        default_playlist_image(username, name).await?;

        Ok(())
    }

    pub async fn set_playlist_image(
        &self,
        username: u64,
        name: &str,
        image: &str,
    ) -> anyhow::Result<()> {
        save_playlist_image(username, name, image.to_string()).await?;

        Ok(())
    }

    pub async fn remove_playlist_image(&self, username: u64, name: &str) -> anyhow::Result<()> {
        remove_file(&format!(
            "{}/{}-{}.png",
            env_fetch!("CDN_DIR"),
            username,
            hash(name.as_bytes())
        ))
        .await?;

        default_playlist_image(username, name).await?;

        Ok(())
    }

    pub async fn set_playlist_description(
        &self,
        username: u64,
        name: &str,
        description: &str,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            "
UPDATE 
    playlist 
SET 
    description = $3
WHERE 
    username = $1 
    AND name = $2;
            ",
            BigD::from(username),
            name,
            description
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;
        Ok(())
    }

    pub async fn rename_playlist(
        &self,
        username: u64,
        name: &str,
        new_name: &str,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            "
UPDATE 
    playlist 
SET 
    name = $3
WHERE 
    username = $1 
    AND name = $2;
            ",
            BigD::from(username),
            name,
            new_name
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;
        Ok(())
    }

    pub async fn delete_playlist(&self, username: u64, playlist_name: &str) -> anyhow::Result<()> {
        sqlx::query!(
            "
DELETE FROM 
    playlist
WHERE 
    username = $1 
    AND name = $2;
            ",
            BigD::from(username),
            playlist_name // check if valid playlist
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;

        Ok(())
    }

    pub async fn check_if_username_exists_in_auth(&self, username: &str) -> anyhow::Result<bool> {
        let output = sqlx::query_as!(
            Exists,
            "
SELECT EXISTS(
    SELECT 
        1 
    FROM 
        auth 
    WHERE 
        username = $1 
    LIMIT 1
);
            ",
            Some(BigD::from(hash(username.as_bytes())))
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;

        Ok(match output {
            Some(v) => v.exists.unwrap_or_default(),
            None => false,
        })
    }

    pub async fn is_admin(&self, username: u64) -> anyhow::Result<bool> {
        let result = sqlx::query_as!(
            Exists,
            "
SELECT EXISTS(
    SELECT 
        1 
    FROM 
        auth 
    WHERE 
        username = $1 
        AND admin = true 
    LIMIT 1
);
            ",
            BigD::from(username)
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;

        Ok(match result {
            Some(v) => v.exists.unwrap_or_default(),
            None => false,
        })
    }

    pub async fn get_user_data(&self, userhash: u64) -> anyhow::Result<Option<UserData>> {
        let data = sqlx::query_as!(
            UserDataBigD,
            r#"
SELECT 
    (userdata).* 
FROM 
    auth 
WHERE 
    username = $1;
            "#,
            BigD::from(userhash)
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;
        Ok(data.map(|v| v.into()))
    }

    pub async fn update_login_timestamp(&self, userhash: u64) -> anyhow::Result<()> {
        sqlx::query!(
            "
UPDATE 
    auth 
SET 
    last_login = $2 
WHERE 
    username = $1;
                ",
            BigD::from(userhash),
            time!()
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;
        Ok(())
    }

    pub async fn check_if_user_exists_in_auth(
        &self,
        username: &str,
        password: &str,
    ) -> anyhow::Result<(bool, Option<u64>)> {
        let user = UserAuth::new(username, password);
        let output = sqlx::query_as!(
            Exists,
            "
SELECT EXISTS(
    SELECT 
        1 
    FROM 
        auth 
    WHERE 
        username = $1 
        AND password = $2 
    LIMIT 1
);
                ",
            user.username,
            user.password
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;

        Ok(match output {
            Some(v) => match v.exists {
                Some(v) => (v, Some(user.username_u64)),
                None => (false, None),
            },
            None => (false, None),
        })
    }
}
