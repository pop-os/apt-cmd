// Copyright 2021-2022 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

pub use async_fetcher::Fetcher;

use crate::request::Request as AptRequest;

use futures::stream::{Stream, StreamExt};
use std::{path::Path, pin::Pin, sync::Arc};
use thiserror::Error;
use tokio::sync::mpsc;

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

    /// An error occurred fetching package
    Error(FetchError),

    /// The package has been validated
    Validated,
}

#[derive(Debug, Error)]
pub enum FetchError {
    #[error("{}: fetched package had checksum error", package)]
    Checksum {
        package: String,
        source: crate::hash::ChecksumError,
    },

    #[error("{}: download failed", package)]
    Fetch {
        package: String,
        source: async_fetcher::Error,
    },
}

pub struct FetchRequest {
    pub package: AptRequest,
    pub attempt: usize,
}

#[derive(Default)]
pub struct PackageFetcher {
    fetcher: Fetcher<AptRequest>,
    concurrent: usize,
}

impl PackageFetcher {
    pub fn new(fetcher: Fetcher<AptRequest>) -> Self {
        Self {
            fetcher,
            concurrent: 1,
        }
    }

    pub fn connections_per_file(mut self, connections: u16) -> Self {
        self.fetcher = self
            .fetcher
            .connections_per_file(connections);
        self
    }

    pub fn concurrent(mut self, concurrent: usize) -> Self {
        self.concurrent = concurrent;
        self
    }

    pub fn retries(mut self, retries: u16) -> Self {
        self.fetcher = self.fetcher.retries(retries);
        self
    }

    pub fn fetch(
        self,
        packages: impl Stream<Item = Arc<AptRequest>> + Send + Unpin + 'static,
        destination: Arc<Path>,
    ) -> (
        impl std::future::Future<Output = ()> + Send + 'static,
        mpsc::UnboundedReceiver<FetchEvent>,
    ) {
        let (tx, rx) = mpsc::unbounded_channel::<FetchEvent>();
        let (events_tx, mut events_rx) = mpsc::unbounded_channel();

        let input_stream = packages.map(move |package| {
            (
                async_fetcher::Source::new(
                    Arc::from(vec![Box::from(&*package.uri)].into_boxed_slice()),
                    Arc::from(destination.join(&package.name)),
                ),
                package,
            )
        });

        

        let mut fetch_results = self
            .fetcher
            .events(events_tx)
            .build()
            .stream_from(input_stream, self.concurrent.min(1));

        let event_handler = {
            let tx = tx.clone();
            async move {
                while let Some((dest, package, event)) = events_rx.recv().await {
                    match event {
                        async_fetcher::FetchEvent::AlreadyFetched => {
                            let _ = tx.send(FetchEvent::new(package.clone(), EventKind::Fetched));
                            let _ = tx.send(FetchEvent::new(package, EventKind::Validated));
                        }

                        async_fetcher::FetchEvent::Fetching => {
                            let _ = tx.send(FetchEvent::new(package, EventKind::Fetching));
                        }

                        async_fetcher::FetchEvent::Fetched => {
                            let _ = tx.send(FetchEvent::new(package.clone(), EventKind::Fetched));
                            let tx = tx.clone();

                            tokio::task::spawn_blocking(move || {
                                let event = match crate::hash::compare_hash(
                                    &dest,
                                    package.size,
                                    &package.checksum,
                                ) {
                                    Ok(()) => EventKind::Validated,
                                    Err(source) => {
                                        let _ = std::fs::remove_file(&dest);
                                        EventKind::Error(FetchError::Checksum {
                                            package: package.uri.clone(),
                                            source,
                                        })
                                    }
                                };

                                let _ = tx.send(FetchEvent::new(package, event));
                            });
                        }

                        _ => (),
                    }
                }
            }
        };

        let fetcher = async move {
            while let Some((dest, package, result)) = fetch_results.next().await {
                if let Err(source) = result {
                    let _ = tx.send(FetchEvent::new(
                        package.clone(),
                        EventKind::Error(FetchError::Fetch {
                            package: package.uri.clone(),
                            source,
                        }),
                    ));

                    let _ = tokio::fs::remove_file(&dest).await;
                }
            }
        };

        let future = async move {
            let _ = futures::future::join(event_handler, fetcher).await;
        };

        (future, rx)
    }
}
