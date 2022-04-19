use seanify::run;
use std::env;

/*
 * Where all the fun begins
 */

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    // the run function in lib.rs has all the goodies
    run(&args).await?;
    Ok(())
}
