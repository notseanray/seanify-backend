use crate::BigD;
use serde::Serialize;

// To insert into postgres they must be of all optional type which is slightly inconvient

/*
 * This is part of the userdata even though it does store a song name, so I've given it slightly
 * different names to tell them apart easily
 */
#[derive(Default)]
pub(crate) struct SongInPlaylist {
    pub username: u64,
    playlist_name: String,
    pub song_hash: BigD,
    pub song_name: String,
    pub date_added: BigD,
    pub custom_name: Option<String>, // limit to 100 char
}

#[derive(Default)]
pub(crate) struct Playlist {
    pub username: u64,
    creation_timestamp: u64,
    pub name: String,                // limit to 30 char
    pub description: Option<String>, // limit to 100 char
    pub image: Option<String>,       // limit to 200 char
    pub public_playlist: bool,
    pub last_update: u64,
}

#[derive(Default, Serialize)]
pub(crate) struct UserData {
    pub public_profile: Option<bool>,
    pub display_name: Option<String>, //limit to 30 char
    pub share_status: Option<bool>,
    pub now_playing: Option<String>, // keep under 50 char
    pub public_status: Option<String>,
    pub recent_plays: Option<Vec<String>>,
    pub followers: Option<Vec<String>>, // vec of display_name
    pub following: Option<Vec<String>>,
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

impl UserData {
    pub(crate) fn new() -> Self {
        Self {
            public_profile: Some(false),
            display_name: None,
            share_status: Some(false),
            now_playing: None,
            public_status: None,
            recent_plays: Some(Vec::new()),
            followers: Some(Vec::new()),
            following: Some(Vec::new())
        }
    }
}
