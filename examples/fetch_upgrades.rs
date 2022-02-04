use anyhow::Context;
use apt_cmd::{
    fetch::{EventKind, PackageFetcher},
    AptGet,
};

use std::{path::Path, sync::Arc};
use tokio_stream::wrappers::ReceiverStream;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    const CONCURRENT_FETCHES: usize = 4;
    const DELAY_BETWEEN: u64 = 100;
    const RETRIES: u32 = 3;

    let client = isahc::HttpClient::new().unwrap();
    let path = Path::new("./packages/");
    let partial = path.join("partial");
    let (fetch_tx, fetch_rx) = tokio::sync::mpsc::channel(CONCURRENT_FETCHES);
    let packages = ReceiverStream::new(fetch_rx);

    if !path.exists() {
        tokio::fs::create_dir_all(path).await.unwrap();
    }

    let (fetcher, mut events) = PackageFetcher::new(client)
        .concurrent(CONCURRENT_FETCHES)
        .delay_between(DELAY_BETWEEN)
        .retries(RETRIES)
        .fetch(packages, Arc::from(path), Arc::from(partial));

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
            println!("Event: {:#?}", event);

            if let EventKind::Error(why) = event.kind {
                return Err(why).context("package fetching failed");
            }
        }

        Ok::<(), anyhow::Error>(())
    };

    let fetcher = async move {
        fetcher.await;
        Ok(())
    };

    futures::try_join!(fetcher, sender, receiver)?;

    Ok(())
}
