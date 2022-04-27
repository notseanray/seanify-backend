use crate::env_fetch;
use anyhow::anyhow;
use image::DynamicImage::ImageRgba8;
use image::{GenericImageView, ImageBuffer};
use log::error;
use rand::{thread_rng, Rng};
use seahash::hash;
use std::env;
use tokio::fs::create_dir;

// images are squares, they must be rescaled on the client to this size before sending
const IMAGE_SIZE: usize = 400;

pub(crate) async fn save_base64(fname: &str, data: String) -> anyhow::Result<()> {
    // remove the header if we need to
    let data = data.replace("data:image/png;base64,", "");

    let data = match base64::decode(data) {
        Ok(v) => v,
        Err(_) => return Err(anyhow!("InvalidBase64")),
    };

    let img = image::load_from_memory(&data).unwrap();
    let image = ImageRgba8(img.to_rgba8());

    if (IMAGE_SIZE as u32, IMAGE_SIZE as u32) != image.dimensions() {
        return Err(anyhow!("InvalidDimensions"));
    }

    match image.save(&format!("{}/{fname}.png", env_fetch!("CDN_DIR"))) {
        Ok(_) => Ok(()),
        Err(_) => Err(anyhow!("FailedToSave")),
    }
}

pub(crate) async fn save_pfp(userhash: u64, data: String) -> anyhow::Result<()> {
    save_base64(&userhash.to_string(), data).await
}

pub(crate) async fn save_playlist_image(
    userhash: u64,
    playlistname: &str,
    data: String,
) -> anyhow::Result<()> {
    let _ = create_dir(env_fetch!("CDN_DIR")).await;
    // hash the name so that we don't have to deal with weird names causing an epic RCE
    save_base64(
        &format!("{userhash}-{}.png", hash(playlistname.as_bytes())),
        data,
    )
    .await
}

pub(crate) async fn default_playlist_image(
    username: u64,
    playlistname: &str,
) -> anyhow::Result<()> {
    let playlist_hash = hash(playlistname.as_bytes());
    default_image(playlist_hash, &format!("{username}-{playlist_hash}")).await
}

pub(crate) async fn default_pfp(username: u64) -> anyhow::Result<()> {
    default_image(username, &username.to_string()).await
}

async fn default_image(hash: u64, fname: &str) -> anyhow::Result<()> {
    let mut rng = thread_rng();
    let grid = rng.gen_range(3..=6);

    // randomly generate a grid
    let mut indexes = Vec::with_capacity(grid * grid);
    let mut at_least_one = false;

    // this is really awful
    // make sure at least one is true
    while !at_least_one {
        for _ in 0..grid * grid {
            let square = rng.gen_bool(0.6);
            if square {
                at_least_one = true;
            }
            indexes.push(square);
        }
    }

    let mut image_data: Vec<u8> = Vec::with_capacity(640000);
    let (r, g, b, o) = (hash % 120 * 2, hash % 220, hash % 60 * 3, hash % 6 * 40);

    //let split = 160000 / grid;
    for i in 0..160000 {
        let x = (i % IMAGE_SIZE) / (IMAGE_SIZE / grid);
        let y = (i / IMAGE_SIZE) / (IMAGE_SIZE / grid);
        if indexes[y.clamp(0, grid - 1) * grid + x.clamp(0, grid - 1)] {
            image_data.push(r.clamp(0, 255) as u8);
            image_data.push(g.clamp(0, 255) as u8);
            image_data.push(b.clamp(0, 255) as u8);
            image_data.push(o.clamp(160, 255) as u8);
            continue;
        }
        for _ in 0..4 {
            image_data.push((hash % 255) as u8);
        }
    }

    let pfp = image::DynamicImage::ImageRgba8(
        match ImageBuffer::from_raw(IMAGE_SIZE as u32, IMAGE_SIZE as u32, image_data) {
            Some(v) => v,
            None => return Ok(()),
        },
    );

    pfp.save(&format!("{}/{fname}.png", env_fetch!("CDN_DIR")))?;

    Ok(())
}
