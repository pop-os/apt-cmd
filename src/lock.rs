// Copyright 2021-2022 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

use async_stream::stream;
use futures::stream::{Stream, StreamExt};
use std::path::Path;
use std::time::Duration;
use tokio::time::sleep;

const LISTS_LOCK: &str = "/var/lib/apt/lists/lock";
const DPKG_LOCK: &str = "/var/lib/dpkg/lock";
pub enum AptLockEvent {
    Locked,
    Unlocked,
}

pub async fn apt_lock_wait() {
    let stream = apt_lock_watch();
    futures::pin_mut!(stream);

    while stream.next().await.is_some() {}
}

pub fn apt_lock_watch() -> impl Stream<Item = AptLockEvent> {
    stream! {
        let paths = &[Path::new(DPKG_LOCK), Path::new(LISTS_LOCK)];

        let mut waiting = apt_lock_found(paths);

        if (waiting) {
            yield AptLockEvent::Locked;
            while waiting {
                sleep(Duration::from_secs(3)).await;
                waiting = apt_lock_found(paths);
            }
        }

        yield AptLockEvent::Unlocked;
    }
}

#[must_use]
pub fn apt_lock_found(paths: &[&Path]) -> bool {
    use procfs::process::{all_processes, FDTarget};

    let processes = match all_processes() {
        Ok(processes) => processes,
        Err(_) => return false,
    };

    for proc in processes.filter_map(Result::ok) {
        let Ok(fdinfos) = proc.fd() else {
            continue
        };

        for fdinfo in fdinfos.filter_map(Result::ok) {
            if let FDTarget::Path(path) = fdinfo.target {
                if paths.iter().any(|&p| &*path == p) {
                    return true;
                }
            }
        }
    }

    false
}
