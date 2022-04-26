use photon_rs::{native::save_image, native::open_image};
use log::error;
use std::env;
use crate::env_fetch;

pub(crate) fn encode_pfp(userhash: u64) -> anyhow::Result<String> {
    let image = open_image(&format!("{}/{userhash}", env_fetch!("CDN_DIR")))?;
    Ok(image.get_base64())
}

pub(crate) fn save_pfp(userhash: u64, data: &str) -> anyhow::Result<()> {
    let image = photon_rs::base64_to_image(data);
    save_image(image, &format!("./cdn/{userhash}"));
    Ok(())
}

pub(crate) fn default_pfp(userhash: u64) {

}
