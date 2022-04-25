use crate::env_fetch;
use crate::BigD;
use log::error;
use num_traits::cast::ToPrimitive;
use serde::{Deserialize, Serialize};
use std::env;
use tokio::fs::create_dir;

// To insert into postgres they must be of all optional type which is slightly inconvient

/*
 * This is part of the userdata even though it does store a song name, so I've given it slightly
 * different names to tell them apart easily
 */
#[derive(Default)]
pub(crate) struct SongInPlaylist {
    pub username: u64,
    pub playlist_name: String,
    pub song_hash: BigD,
    pub song_name: String,
    pub date_added: BigD,
    pub custom_name: Option<String>, // limit to 100 char
}

#[derive(Default, Deserialize)]
pub(crate) struct Playlist {
    pub username: u64,
    pub name: String, // limit to 30 char
    pub creation_timestamp: u64,
    pub description: Option<String>, // limit to 100 char
    pub image: Option<String>,       // limit to 200 char
    pub public_playlist: bool,
    pub last_update: u64,
}

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct UserData {
    pub public_profile: Option<bool>,
    pub profile_picture: Option<String>,
    pub display_name: Option<String>, //limit to 30 char
    pub share_status: Option<bool>,
    pub now_playing: Option<String>, // keep under 50 char
    pub public_status: Option<String>,
    pub recent_plays: Option<Vec<String>>,
    pub followers: Option<Vec<u64>>, // vec of display_name
    pub following: Option<Vec<u64>>,
}

pub(crate) struct UserDataBigD {
    pub public_profile: Option<bool>,
    pub profile_picture: Option<String>,
    pub display_name: Option<String>, //limit to 30 char
    pub share_status: Option<bool>,
    pub now_playing: Option<String>, // keep under 50 char
    pub public_status: Option<String>,
    pub recent_plays: Option<Vec<String>>,
    pub followers: Option<Vec<BigD>>, // vec of display_name
    pub following: Option<Vec<BigD>>,
}

impl Into<UserData> for UserDataBigD {
    fn into(self) -> UserData {
        UserData {
            public_profile: self.public_profile,
            profile_picture: self.profile_picture,
            display_name: self.display_name,
            share_status: self.share_status,
            now_playing: self.now_playing,
            public_status: self.public_status,
            recent_plays: self.recent_plays,
            followers: Some(
                self.followers
                    .unwrap_or_default()
                    .iter()
                    .map(|x| x.to_u64().unwrap_or(0))
                    .collect::<Vec<u64>>(),
            ),
            following: Some(
                self.following
                    .unwrap_or_default()
                    .iter()
                    .map(|x| x.to_u64().unwrap_or(0))
                    .collect::<Vec<u64>>(),
            ),
        }
    }
}

impl UserData {
    pub(crate) fn new() -> Self {
        Self {
            public_profile: Some(false),
            profile_picture: None,
            display_name: None,
            share_status: Some(false),
            now_playing: None,
            public_status: None,
            recent_plays: Some(Vec::new()),
            followers: Some(Vec::new()),
            following: Some(Vec::new()),
        }
    }
    pub(crate) async fn save_playlist_image(&mut self, base64: &str) -> anyhow::Result<()> {
        let _ = create_dir(env_fetch!("CDN_DIR")).await;
        Ok(())
    }
}
