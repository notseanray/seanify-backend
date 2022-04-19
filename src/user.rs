use crate::BigD;
use serde::{Deserialize, Serialize};
use serde_json::Result;

// To insert into postgres they must be of all optional type which is slightly inconvient

/*
 * This is part of the userdata even though it does store a song name, so I've given it slightly
 * different names to tell them apart easily 
 */
#[derive(Serialize, Deserialize)]
pub(crate) struct SongInPlaylist {
    song_hash: Option<u64>,
    date_added: Option<u64>,
    custom_name: Option<String> // limit to 100 char
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Playlist {
    name: Option<String>, // limit to 30 char
    description: Option<String>, // limit to 100 char
    public: Option<bool>,
    last_update: Option<u64>,
    songs: Option<Vec<SongInPlaylist>>
}

#[derive(Serialize, Deserialize)]
pub(crate) struct UserData {
    public: Option<bool>,
    playlist: Option<Vec<Playlist>>,
    display_name: Option<String>, //limit to 30 char
    share_status: Option<bool>,
    now_playing: Option<String>, // keep under 50 char
    recent_plays: Option<Vec<String>>,
    followers: Option<Vec<String>>, // vec of display_name
    following: Option<Vec<String>>
}

pub(crate) struct AuthData {
    _username: Option<BigD>,
    _password: Option<BigD>,
    userdata: Option<UserData>,
    admin: Option<bool>
}

/*
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
*/

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

}
