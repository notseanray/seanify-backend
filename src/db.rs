use log::{error, info, LevelFilter};
use seahash::hash;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, Postgres};
use sqlx::ConnectOptions;
use sqlx::Pool;
use std::env::var;
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    pub exists: Option<bool>
}

// Hash the username and password then return, update the last login time
impl UserAuth {
    pub fn new(username: &str, password: &str) -> Self {
        let name = hash(username.as_bytes());
        Self {
            username: Some(name.into()),
            username_u64: name,
            password: Some(hash(password.as_bytes()).into()),
            last_login: Some(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
                    .into(),
            ),
        }
    }
}

pub(crate) struct Database {
    pub database: Pool<Postgres>
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
                    "{} is invalid due to: {e}, \
using default of {}",
                $val,
                $default
                );
                $default
            }
        }
    };
}

impl Database {
    pub async fn new() -> anyhow::Result<Self> {
        let uri = String::from(var("DATABASE_URL").unwrap());
        Ok(Self {
            database: Self::try_connect(&uri).await
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

    pub async fn insert_song(&self, song: Song) -> anyhow::Result<()> {
        sqlx::query!(
            "
 INSERT INTO songs(id, title, upload_date, uploader, url, genre,\
 thumbnail, album, album_artist, artist, creator, filesize, downloaded)
 VALUES($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13);
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
            true
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;

        Ok(())
    }

    pub async fn new_user(&self, username: &str, password: &str) -> anyhow::Result<()> {
        if let Ok((true, _)) = self.check_if_user_exists_in_auth(username, password).await {
            // TODO ERROR CODES
            // already exists in table
            return Ok(());
        }
        let user = UserAuth::new(username, password);
        sqlx::query!(
            "
INSERT INTO auth(username, password, last_login)
VALUES($1, $2, $3);
            ",
            user.username,
            user.password,
            user.last_login
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;

        Ok(())
    }

    pub async fn check_if_username_exists_in_auth(&self, username: &str) -> anyhow::Result<bool> {
        let output = sqlx::query_as!(
            Exists,
            "
SELECT EXISTS(SELECT 1 FROM auth WHERE username = $1 LIMIT 1);
            ",
            Some(BigD::from(hash(username.as_bytes())))
        )
        .fetch_optional(&mut self.database.acquire().await?)
        .await?;

        Ok(match output {
            Some(v) => match v.exists {
                Some(v) => v,
                None => false,
            },
            None => false,
        })
    }

    pub async fn is_admin(
        &self,
        username: u64
        ) -> anyhow::Result<bool> {
        let result = sqlx::query_as!(
            Exists,
            "
SELECT EXISTS(SELECT 1 FROM auth WHERE username = $1 AND admin = true LIMIT 1);
            ",
            BigD::from(username)
            ).fetch_optional(&mut self.database.acquire().await?)
            .await?;

        Ok(match result {
            Some(v) => match v.exists {
                Some(v) => v,
                None => false
            },
            None => false
        })
    }

    pub async fn update_login_timestamp(
        &self,
        userhash: u64
    ) -> anyhow::Result<()> {
        sqlx::query!(
                "
UPDATE auth SET last_login = $2 
WHERE username = $1
                ",
                BigD::from(userhash),
                BigD::from(SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
                )
            ).fetch_optional(&mut self.database.acquire().await?)
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
SELECT EXISTS(SELECT 1 FROM auth WHERE username = $1 AND password = $2 LIMIT 1);
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
