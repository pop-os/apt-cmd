use async_io::Timer;
use async_stream::stream;
use futures::stream::{Stream, StreamExt};
use futures_util::pin_mut;
use std::path::Path;
use std::time::Duration;

const LISTS_LOCK: &str = "/var/lib/apt/lists/lock";
const DPKG_LOCK: &str = "/var/lib/dpkg/lock";
pub enum AptLockEvent {
    Locked,
    Unlocked,
}

pub async fn apt_lock_wait() {
    let stream = apt_lock_watch();
    pin_mut!(stream);

    while stream.next().await.is_some() {}
}

pub fn apt_lock_watch() -> impl Stream<Item = AptLockEvent> {
    stream! {
        let paths = &[Path::new(DPKG_LOCK), Path::new(LISTS_LOCK)];

        let mut waiting = apt_lock_found(paths);

        if (waiting) {
            yield AptLockEvent::Locked;
            while waiting {
                Timer::after(Duration::from_secs(3)).await;
                waiting = apt_lock_found(paths);
            }
        }

        yield AptLockEvent::Unlocked;
    }
}

pub fn apt_lock_found(paths: &[&Path]) -> bool {
    use procfs::process::{all_processes, FDTarget};

    let processes = match all_processes() {
        Ok(processes) => processes,
        Err(_) => return false,
    };

    for proc in processes {
        if let Ok(fdinfos) = proc.fd() {
            for fdinfo in fdinfos {
                if let FDTarget::Path(path) = fdinfo.target {
                    if paths.iter().any(|&p| &*path == p) {
                        return true;
                    }
                }
            }
        }
    }

    false
}
