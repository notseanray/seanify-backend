use crate::BigD;
use num_traits::cast::ToPrimitive;
use serde::{Deserialize, Serialize};

// To insert into postgres they must be of all optional type which is slightly inconvient

#[derive(Default, Deserialize, Serialize)]
pub(crate) struct Playlist {
    pub name: String,                // limit to 30 char
    pub description: Option<String>, // limit to 100 char
    pub public_playlist: bool,
}

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct UserData {
    pub public_profile: Option<bool>,
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
    pub display_name: Option<String>, //limit to 30 char
    pub share_status: Option<bool>,
    pub now_playing: Option<String>, // keep under 50 char
    pub public_status: Option<String>,
    pub recent_plays: Option<Vec<String>>,
    pub followers: Option<Vec<BigD>>, // vec of display_name
    pub following: Option<Vec<BigD>>,
}

impl From<UserDataBigD> for UserData {
    fn from(data: UserDataBigD) -> Self {
        Self {
            public_profile: data.public_profile,
            display_name: data.display_name,
            share_status: data.share_status,
            now_playing: data.now_playing,
            public_status: data.public_status,
            recent_plays: data.recent_plays,
            followers: Some(
                data.followers
                    .unwrap_or_default()
                    .iter()
                    .map(|x| x.to_u64().unwrap_or(0))
                    .collect::<Vec<u64>>(),
            ),
            following: Some(
                data.following
                    .unwrap_or_default()
                    .iter()
                    .map(|x| x.to_u64().unwrap_or(0))
                    .collect::<Vec<u64>>(),
            ),
        }
    }
}
