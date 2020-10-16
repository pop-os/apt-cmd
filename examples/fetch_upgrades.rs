use apt_cmd::AptGet;
use futures_lite::stream;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    futures_lite::future::block_on(async move {
        let packages = AptGet::new().noninteractive().upgrade_uris().await??;

        apt_cmd::fetch_upgrades(
            &surf::Client::new(),
            stream::iter(packages),
            Path::new("./packages/"),
        )
        .await
        .unwrap();

        Ok(())
    })
}
