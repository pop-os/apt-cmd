use anyhow::Context;
use apt_cmd::{
    fetch::{EventKind, FetcherExt},
    AptGet,
};

use std::{path::Path, sync::Arc};
use tokio_stream::wrappers::ReceiverStream;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    const CONCURRENT_FETCHES: usize = 8;

    let path = Path::new("./packages/");
    let (fetch_tx, fetch_rx) = tokio::sync::mpsc::channel(CONCURRENT_FETCHES);
    let packages = ReceiverStream::new(fetch_rx);

    if !path.exists() {
        tokio::fs::create_dir_all(path).await.unwrap();
    }

    let (fetcher, mut events) = async_fetcher::Fetcher::default()
        .connections_per_file(4)
        .into_package_fetcher()
        .concurrent(CONCURRENT_FETCHES)
        .fetch(packages, Arc::from(path));

    // Fetch a list of packages that need to be fetched, and send them on their way
    let sender = async move {
        let packages = AptGet::new()
            .noninteractive()
            .fetch_uris(&["full-upgrade"])
            .await
            .context("failed to spawn apt-get command")?
            .context("failed to fetch package URIs from apt-get")?;

        for package in packages {
            let _ = fetch_tx.send(Arc::new(package)).await;
        }

        Ok::<(), anyhow::Error>(())
    };

    // Begin listening for packages to fetch
    let receiver = async move {
        while let Some(event) = events.recv().await {
            eprintln!("{:#?}\n", event);
            if let EventKind::Error(why) = event.kind {
                return Err(why).context("package fetching failed");
            }
        }

        eprintln!("finished");

        Ok::<(), anyhow::Error>(())
    };

    let fetcher = async move {
        fetcher.await;
        Ok(())
    };

    futures::try_join!(fetcher, sender, receiver)?;

    Ok(())
}
