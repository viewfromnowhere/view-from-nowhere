use anyhow::Result;
use nowhere_common::observability::LogConfig;
use nowhere_common::observability::init_logging;
use nowhere_config::{NowhereConfig, NowhereConfigLoader};
use tether::{Tether, build_from_config};
mod tether;

#[tokio::main]
async fn main() -> Result<()> {
    // 1) Load config (env wins)
    let cfg: NowhereConfig = NowhereConfigLoader::new()
        .with_file("nowhere.yaml")
        .load()?;

    //FIXME: Need to set up logging from YAML config file
    init_logging(LogConfig::default())?;

    let mut tether = Tether::new();
    build_from_config(&mut tether, cfg).await?;

    tether.run().await
}
