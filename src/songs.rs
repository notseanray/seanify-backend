use crate::{CACHE_DIR, DB};
use core::fmt;
use log::{info, warn};
use seahash::hash;
use std::{collections::VecDeque, path::PathBuf};
use tokio::{process::Command, fs::create_dir_all};

use youtube_dl::{YoutubeDl, YoutubeDlOutput};

pub(crate) struct Song {
    pub id: Option<u64>,
    pub title: Option<String>,
    pub upload_date: Option<String>,
    pub uploader: Option<String>,
    pub url: Option<String>,
    pub genre: Option<String>,
    pub thumbnail: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub artist: Option<String>,
    pub creator: Option<String>,
    pub filesize: Option<i64>,
    pub downloaded: bool
}

pub enum SongError {
    NotSingleVideo,
    UnableToDownload,
}

impl fmt::Display for SongError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotSingleVideo => write!(f, "Expected single video link! not playlist"),
            Self::UnableToDownload => write!(f, "Unable to download due to network error"),
        }
    }
}


impl Song {
    pub fn new(url: &str) -> anyhow::Result<Self, SongError> {
        let output = match YoutubeDl::new(url)
            .socket_timeout("5")
            .extra_arg("--max-filesize")
            .extra_arg("10m")
            .extract_audio(true)
            .extra_arg("--retries")
            .extra_arg("3")
            .extra_arg("--audio-format")
            .extra_arg("mp3")
            .extra_arg("--embed-thumbnail")
            .run()
            {
                Ok(v) => v,
                Err(_) => return Err(SongError::UnableToDownload)
            };

        match output {
            YoutubeDlOutput::SingleVideo(v) => {
                Ok(Self {
                    // TODO PROPER ERR HANDLING HERE
                    // TODO HASH ARTIST + TITLE + DATE
                    id: Some(hash(v.title.as_bytes())),
                    title: Some(v.title),
                    upload_date: v.upload_date,
                    uploader: v.uploader,
                    url: v.url,
                    genre: v.genre,
                    thumbnail: v.thumbnail,
                    album: v.album,
                    album_artist: v.album_artist,
                    artist: v.artist,
                    creator: v.creator,
                    filesize: v.filesize,
                    downloaded: false
                })
            }
            _ => Err(SongError::NotSingleVideo),
        }
    }
}

pub(crate) struct SongManager {
    download_queue: VecDeque<String>,
    hourly_ytdl_call_max: (u64, Option<u64>),
    hourly_bandwidth_limit_mb: (u64, Option<u64>),
    max_file_size_mb: Option<u64>,
}

pub enum SongManagerError {
    RateLimitYtdlCall,
    RateLimitBandwidthMB,
    MaxFileSizeLimit,
    QueueLimit,
    InvalidSong,
    MaxCacheSizeLimit
}

impl fmt::Display for SongManagerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RateLimitYtdlCall=> write!(f, "Expected single video link! not playlist"),
            Self::RateLimitBandwidthMB => write!(f, "Unable to download due to network error"),
            Self::MaxFileSizeLimit => write!(f, "Max file size limit reached"),
            Self::QueueLimit => write!(f, "Queue limit reached"),
            Self::InvalidSong => write!(f, "Provided with invalid song"),
            Self::MaxCacheSizeLimit => write!(f, "Max Cache Limit Reached"),
        }
    }
}

impl SongManager {
    pub fn new(
        hourly_ytdl_call_max: Option<u64>,
        hourly_bandwidth_limit_mb: Option<u64>,
        max_file_size_mb: Option<u64>,
    ) -> Self {
        Self {
            download_queue: VecDeque::with_capacity(1),
            hourly_ytdl_call_max: (0, hourly_ytdl_call_max),
            hourly_bandwidth_limit_mb: (0, hourly_bandwidth_limit_mb),
            max_file_size_mb,
        }
    }

    pub fn request(&mut self, url: String) {
        self.download_queue.push_back(url);
    }

    pub fn _clear_queue(&mut self) {
        self.download_queue.clear();
    }

    pub async fn cycle_queue(&mut self) -> anyhow::Result<(), SongManagerError> {
        // MOVE THIS
        if let Some(v) = self.hourly_bandwidth_limit_mb.1 {
            if self.hourly_bandwidth_limit_mb.0 > v * 1024 {
                return Err(SongManagerError::RateLimitBandwidthMB);
            }
        }

        if let Some(v) = self.hourly_ytdl_call_max.1 {
            if self.hourly_ytdl_call_max.0 >= v {
                return Err(SongManagerError::RateLimitYtdlCall); 
            }
        }

        if let Some(url) = self.download_queue.pop_front() {
            let mut song = match Song::new(&url) {
                Ok(v) => v,
                Err(_) => return Err(SongManagerError::InvalidSong),
            };
            if song.title.is_none() || song.title.clone().unwrap_or_default().len() < 1 {
                return Ok(());
            }
            self.hourly_ytdl_call_max.0 += 1;
            if let Some(config_max) = self.max_file_size_mb {
                if let Some(video_size) = song.filesize {
                    if config_max * 1024 < video_size as u64 {
                        warn!("song surpassed max set file size!");
                        return Ok(());
                    }
                }
            }

            // create cache directory if it doesn't exist
            if PathBuf::from(CACHE_DIR.to_string()).exists() {
                let _ = create_dir_all(CACHE_DIR.to_string()).await;
            }

            let cmd = Command::new("aria2c")
                .args([
                    "-d",
                    &CACHE_DIR,
                    "-o",
                    &song.id.unwrap().to_string(),
                    &song.url.clone().unwrap(),
                ]) // TODO error handling here
                .status()
                .await;

            match cmd {
                Ok(_) => {
                    info!("downloaded song");
                    song.downloaded = true;
                    // FIX
                    DB.get().await.insert_song(song).await.unwrap();
                }
                Err(_) => return Ok(()),
            };
        }
        Ok(())
    }
}
