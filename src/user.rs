use crate::BigD;
use std::error::Error;
use sqlx::{Database, Decode};
use sqlx::database::HasValueRef;
use serde_json::Result;

// To insert into postgres they must be of all optional type which is slightly inconvient

/*
 * This is part of the userdata even though it does store a song name, so I've given it slightly
 * different names to tell them apart easily 
 */
#[derive(sqlx::FromRow)]
pub(crate) struct SongInPlaylist {
    pub song_hash: Option<BigD>,
    pub date_added: Option<BigD>,
    pub custom_name: Option<String> // limit to 100 char
}

#[derive(sqlx::FromRow)]
pub(crate) struct Playlist {
    pub name: Option<String>, // limit to 30 char
    pub description: Option<String>, // limit to 100 char
    pub image: Option<String>, // limit to 200 char
    pub public_playlist: Option<bool>,
    pub last_update: Option<u64>,
    pub playlist_songs: Option<Vec<SongInPlaylist>>
}

#[derive(sqlx::FromRow)]
pub(crate) struct UserData {
    pub public_profile: Option<bool>,
    pub playlist: Option<Vec<Playlist>>,
    pub display_name: Option<String>, //limit to 30 char
    pub share_status: Option<bool>,
    pub now_playing: Option<String>, // keep under 50 char
    pub public_status: Option<String>,
    pub recent_plays: Option<Vec<String>>,
    pub followers: Option<Vec<String>>, // vec of display_name
    pub following: Option<Vec<String>>
}

#[derive(sqlx::FromRow)]
pub(crate) struct AuthData {
    pub username: BigD,
    pub password: BigD,
    pub admin: bool,
    pub last_login: Option<BigD>,
    pub userdata: Option<UserData>
}

impl<'r, DB: Database> Decode<'r, DB> for AuthData
where
    &'r str: Decode<'r, DB>
{
    fn decode(
        value: <DB as HasValueRef<'r>>::ValueRef,
    ) -> Result<Self, Box<dyn Error + 'static + Send + Sync>> {
        // the interface of ValueRef is largely unstable at the moment
        // so this is not directly implementable

        // however, you can delegate to a type that matches the format of the type you want
        // to decode (such as a UTF-8 string)

        let value = <&str as Decode<DB>>::decode(value)?;

        // now you can parse this into your type (assuming there is a `FromStr`)

        Ok(value.parse()?)
    }
}

macro_rules! truncate {
    ($val:expr, $len:expr) => {
        match $val {
            Some(v) => Some(match v.len() > $len {
                true => {
                    let mut new_input = String::with_capacity($len);
                    let shortened = &v.chars().collect::<Vec<char>>()[..$len];
                    shortened.iter().for_each(|x| new_input.push(*x));
                    new_input
                }
                false => v,
            }),
            None => None,
        }
    };
}

/*
impl UserData {
    pub(crate) fn new() -> Self {
        Self {
            public: Some(false),
            playlist: None,
            display_name: None,
            share_status: Some(false),
            now_playing: None,
            recent_plays: Some(Vec::new()),
            followers: Some(Vec::new()),
            following: Some(Vec::new())
        }
    }
}*/
