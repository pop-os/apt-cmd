#[macro_use]
extern crate derive_more;

pub mod apt_uri;
pub mod lock;

use crate::apt_uri::{AptUri, AptUriError};
use as_result::*;
use async_process::{Child, ChildStdout, Command, ExitStatus, Stdio};
use async_stream::stream;
use futures_lite::io::BufReader;
use futures_lite::prelude::*;
use futures_lite::stream::{Boxed as BoxedStream, StreamExt};
use futures_util::pin_mut;
use std::{collections::HashSet, io, path::Path};
use surf::http::{Request, Url};

#[derive(Debug)]
pub enum UpdateEvent {
    BadPPA(BadPPA),
    ExitStatus(io::Result<ExitStatus>),
}

#[derive(Debug)]
pub struct BadPPA {
    pub url: String,
    pub pocket: String,
}

#[derive(AsMut, Deref, DerefMut)]
#[as_mut(forward)]
pub struct AptGet(Command);

impl AptGet {
    pub fn new() -> Self {
        let mut cmd = Command::new("apt-get");
        cmd.env("LANG", "C");
        Self(cmd)
    }

    pub fn allow_downgrades(mut self) -> Self {
        self.arg("--allow-downgrades");
        return self;
    }

    pub fn force(mut self) -> Self {
        self.arg("-y");
        return self;
    }

    pub fn noninteractive(mut self) -> Self {
        self.env("DEBIAN_FRONTEND", "noninteractive");
        return self;
    }

    pub async fn upgrade_uris(mut self) -> io::Result<Result<HashSet<AptUri>, AptUriError>> {
        lock::apt_lock_wait().await;

        self.args(&["--print-uris", "full-upgrade"]);

        let (mut child, stdout) = self.spawn_with_stdout()?;

        let stdout = BufReader::new(stdout).lines();

        pin_mut!(stdout);

        let mut packages = HashSet::new();

        while let Some(Ok(line)) = stdout.next().await {
            if !line.starts_with('\'') {
                continue;
            }

            let package = match line.parse::<AptUri>() {
                Ok(package) => package,
                Err(why) => return Ok(Err(why)),
            };

            packages.insert(package);
        }

        child.status().await.map_result()?;

        Ok(Ok(packages))
    }

    pub async fn update(mut self) -> io::Result<BoxedStream<UpdateEvent>> {
        lock::apt_lock_wait().await;

        self.arg("update");

        let (mut child, stdout) = self.spawn_with_stdout()?;

        let stdout = BufReader::new(stdout).lines();

        let stream = stream! {
            pin_mut!(stdout);
            while let Some(Ok(line)) = stdout.next().await {
                if line.starts_with("Err") {
                    let mut fields = line.split_ascii_whitespace();
                    let _ = fields.next();
                    let url = fields.next().unwrap();
                    let pocket = fields.next().unwrap();

                    yield UpdateEvent::BadPPA(BadPPA {
                        url: url.into(),
                        pocket: pocket.into(),
                    });
                }
            }

            yield UpdateEvent::ExitStatus(child.status().await);
        };

        Ok(stream.boxed())
    }

    pub fn spawn_with_stdout(mut self) -> io::Result<(Child, ChildStdout)> {
        self.stdout(Stdio::piped());
        self.stderr(Stdio::null());
        self.spawn().map(|mut child| {
            let stdout = child.stdout.take().unwrap();
            (child, stdout)
        })
    }
}

pub async fn fetch_upgrades<P: Stream<Item = AptUri>>(
    client: &surf::Client,
    packages: P,
    path: &Path,
) -> Result<(), surf::Error> {
    pin_mut!(packages);
    while let Some(uri) = packages.next().await {
        let mut resp = client
            .send(Request::get(Url::parse(&uri.uri).unwrap()))
            .await?;

        let dest = path.join(&uri.name);

        let parent = dest.parent();
        if let Some(parent) = parent {
            async_fs::create_dir_all(parent).await.unwrap();
        }

        let mut file = async_fs::File::create(&dest).await.unwrap();

        futures_lite::io::copy(&mut resp, &mut file).await.unwrap();
    }

    Ok(())
}
