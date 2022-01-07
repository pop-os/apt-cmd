// Copyright 2021 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

use crate::request::Request as AptRequest;

use async_io::Timer;
use futures::stream::{Stream, StreamExt};
use isahc::http::Request;
use std::{io, path::Path, pin::Pin, sync::Arc, time::Duration};
use thiserror::Error;

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
    Fetched,

    /// An error occurred fetching or validating package
    Error(FetchError),

    /// The package has been validated
    Validated,
}

#[derive(Debug, Error)]
pub enum FetchError {
    #[error("failure when writing package to disk")]
    Copy(#[source] io::Error),

    #[error("failed to create file for package")]
    Create(#[source] io::Error),

    #[error("computed checksum is invalid")]
    InvalidHash(#[source] crate::hash::ChecksumError),

    #[error("failed to rename fetched file")]
    Rename(#[source] io::Error),

    #[error("failed to request package")]
    Request(#[from] Box<dyn std::error::Error + 'static + Sync + Send>),

    #[error("server responded with error: {0}")]
    Response(isahc::http::StatusCode),
}

pub struct FetchRequest {
    pub package: AptRequest,
    pub attempt: usize,
}

pub struct PackageFetcher {
    client: isahc::HttpClient,
    concurrent: usize,
    delay: Option<u64>,
    retries: u32,
}

impl PackageFetcher {
    pub fn new(client: isahc::HttpClient) -> Self {
        Self {
            client,
            concurrent: 1,
            delay: None,
            retries: 3,
        }
    }

    pub fn concurrent(mut self, concurrent: usize) -> Self {
        self.concurrent = concurrent;
        self
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
        partial: Arc<Path>,
        destination: Arc<Path>,
    ) -> FetchEvents {
        let (tx, rx) = flume::bounded(0);

        let Self {
            client,
            concurrent,
            delay,
            retries,
        } = self;

        let client = Arc::new(client);

        let barrier = Arc::new(async_barrier::Barrier::new(1));

        let fetcher =
            packages.for_each_concurrent(Some(concurrent), move |uri: Arc<AptRequest>| {
                let client = client.clone();
                let partial = partial.clone();
                let destination = destination.clone();
                let tx = tx.clone();
                let barrier = barrier.clone();

                smolscale::spawn(async move {
                    let event_process = || async {
                        let _ = tx.send(FetchEvent::new(uri.clone(), EventKind::Fetching));

                        // If defined, ensures that only one thread may initiate a connection at a time.
                        // Prevents us from hammering the servers on release day.
                        if let Some(delay) = delay {
                            let guard = barrier.wait().await;
                            Timer::after(Duration::from_millis(delay)).await;
                            drop(guard);
                        }

                        let mut resp = match client
                            .send_async(Request::get(&uri.uri).body(()).unwrap())
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

                        let partial_file = partial.join(&uri.name);

                        let mut file = async_fs::File::create(&partial_file)
                            .await
                            .map_err(|why| EventKind::Error(FetchError::Create(why)))?;

                        futures::io::copy(resp.body_mut(), &mut file)
                            .await
                            .map_err(|why| EventKind::Error(FetchError::Copy(why)))?;

                        let _ = tx.send(FetchEvent::new(uri.clone(), EventKind::Fetched));

                        crate::hash::compare_hash(&partial_file, uri.size, &uri.checksum)
                            .await
                            .map_err(|why| EventKind::Error(FetchError::InvalidHash(why)))?;

                        let _ = tx.send(FetchEvent::new(uri.clone(), EventKind::Validated));

                        async_fs::rename(&partial_file, &destination.join(&uri.name))
                            .await
                            .map_err(|why| EventKind::Error(FetchError::Rename(why)))?;

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

        smolscale::spawn(fetcher).detach();

        Box::pin(rx.into_stream())
    }
}
