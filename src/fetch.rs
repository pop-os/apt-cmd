use crate::request::Request as AptRequest;

use async_global_executor::spawn;
use async_io::Timer;
use futures::stream::{Stream, StreamExt};
use std::{
    future::Future,
    io,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    time::Duration,
};
use surf::http::{Request, Url};
use thiserror::Error;

pub type Fetcher = Pin<Box<dyn Future<Output = ()> + Send>>;
pub type FetchEvents = Pin<Box<dyn Stream<Item = FetchEvent>>>;

#[derive(Debug)]
pub struct FetchEvent {
    pub package: Arc<AptRequest>,
    pub kind: EventKind,
}

impl FetchEvent {
    pub fn new(package: Arc<AptRequest>, kind: EventKind) -> Self {
        Self { package, kind }
    }
}

#[derive(Debug)]
pub enum EventKind {
    /// Request to download package is being initiated
    Fetching,

    /// Package was downloaded successfully
    Fetched(PathBuf),

    /// An error occurred fetching or validating package
    Error(FetchError),

    /// The package has been validated
    Validated(PathBuf),
}

#[derive(Debug, Error)]
pub enum FetchError {
    #[error("failure when writing package to disk")]
    Copy(#[source] io::Error),

    #[error("failed to create file for package")]
    Create(#[source] io::Error),

    #[error("computed md5sum is invalid")]
    InvalidHash(#[source] crate::hash::ChecksumError),

    #[error("failed to request package")]
    Request(#[from] Box<dyn std::error::Error + 'static + Sync + Send>),

    #[error("server responded with error: {}", _0)]
    Response(surf::StatusCode),
}

pub struct FetchRequest {
    pub package: AptRequest,
    pub attempt: usize,
}

pub struct PackageFetcher {
    client: surf::Client,
    concurrent: usize,
    delay: Option<u64>,
    retries: u32,
}

impl PackageFetcher {
    pub fn new(client: surf::Client) -> Self {
        Self {
            client,
            concurrent: 1,
            delay: None,
            retries: 3,
        }
    }

    pub fn concurrent(mut self, concurrent: usize) -> Self {
        self.concurrent = concurrent;
        return self;
    }

    pub fn delay_between(mut self, delay: u64) -> Self {
        self.delay = Some(delay);
        self
    }

    pub fn retries(mut self, retries: u32) -> Self {
        self.retries = retries;
        self
    }

    pub fn fetch(
        self,
        packages: impl Stream<Item = Arc<AptRequest>> + Send + 'static,
        path: Arc<Path>,
    ) -> (Fetcher, FetchEvents) {
        let (tx, rx) = flume::bounded(0);

        let Self {
            client,
            concurrent,
            delay,
            retries,
        } = self;

        let barrier = Arc::new(async_barrier::Barrier::new(1));

        let fetcher =
            packages.for_each_concurrent(Some(concurrent), move |uri: Arc<AptRequest>| {
                let client = client.clone();
                let path = path.clone();
                let tx = tx.clone();
                let barrier = barrier.clone();

                spawn(async move {
                    let event_process = || async {
                        let _ = tx.send(FetchEvent::new(uri.clone(), EventKind::Fetching));

                        // If defined, ensures that one one thread may initiate a connection at a time.
                        // Prevents us from hammering the servers on release day.
                        if let Some(delay) = delay {
                            let guard = barrier.wait().await;
                            Timer::after(Duration::from_millis(delay)).await;
                            drop(guard);
                        }

                        let mut resp = match client
                            .send(Request::get(Url::parse(&uri.uri).unwrap()))
                            .await
                        {
                            Ok(resp) => resp,
                            Err(why) => {
                                return Err(EventKind::Error(FetchError::Request(Box::from(why))));
                            }
                        };

                        let status = resp.status();
                        if !status.is_success() {
                            return Err(EventKind::Error(FetchError::Response(status)));
                        }

                        let dest = path.join(&uri.name);

                        let mut file = match async_fs::File::create(&dest).await {
                            Ok(f) => f,
                            Err(why) => {
                                return Err(EventKind::Error(FetchError::Create(why)));
                            }
                        };

                        if let Err(why) = futures::io::copy(&mut resp, &mut file).await {
                            return Err(EventKind::Error(FetchError::Copy(why)));
                        }

                        let _ = tx.send(FetchEvent::new(
                            uri.clone(),
                            EventKind::Fetched(dest.clone()),
                        ));

                        if let Err(why) =
                            crate::hash::compare_hash(&dest, uri.size, &uri.md5sum).await
                        {
                            return Err(EventKind::Error(FetchError::InvalidHash(why)));
                        }

                        let _ = tx.send(FetchEvent::new(uri.clone(), EventKind::Validated(dest)));

                        Ok(())
                    };

                    let mut attempts = 0;

                    loop {
                        if let Err(why) = event_process().await {
                            if attempts == retries {
                                let _ = tx.send(FetchEvent::new(uri.clone(), why));
                                break;
                            }

                            attempts += 1;
                            continue;
                        }

                        break;
                    }
                })
            });

        (Box::pin(fetcher), Box::pin(rx.into_stream()))
    }
}
