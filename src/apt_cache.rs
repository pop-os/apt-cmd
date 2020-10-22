use as_result::{IntoResult, MapResult};
use async_process::{Child, ChildStdout, Command};
use futures::io::BufReader;
use futures::prelude::*;
use futures::stream::{Stream, StreamExt};
use futures_util::pin_mut;
use std::io;
use std::pin::Pin;

pub type PackageStream = Pin<Box<dyn Stream<Item = String>>>;

#[derive(AsMut, Deref, DerefMut)]
#[as_mut(forward)]
pub struct AptCache(Command);

impl AptCache {
    pub fn new() -> Self {
        let mut cmd = Command::new("apt-mark");
        cmd.env("LANG", "C");
        Self(cmd)
    }

    pub async fn depends<I, S>(mut self, packages: I) -> io::Result<(Child, PackageStream)>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        self.arg("depends");
        self.args(packages);
        self.stream_packages().await
    }

    pub async fn rdepends<I, S>(mut self, packages: I) -> io::Result<(Child, PackageStream)>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        self.arg("rdepends");
        self.args(packages);
        self.stream_packages().await
    }

    pub async fn predepends_of(package: String) -> io::Result<Vec<String>> {
        let (mut child, mut packages) = AptCache::new().rdepends(&[&package]).await?;

        let mut depends = Vec::new();
        while let Some(package) = packages.next().await {
            depends.push(package);
        }

        child.status().await.map_result()?;

        let (mut child, packages) = AptCache::new().depends(&depends).await?;

        let packages = predepends(packages, package);
        let mut predepends = Vec::new();

        pin_mut!(packages);

        while let Some(package) = packages.next().await {
            predepends.push(package);
        }

        child.status().await.map_result()?;

        Ok(predepends)
    }

    async fn stream_packages(self) -> io::Result<(Child, PackageStream)> {
        let (child, stdout) = self.spawn_with_stdout().await?;

        let mut lines = BufReader::new(stdout).lines().skip(2);

        let stream = async_stream::stream! {
            while let Some(Ok(package)) = lines.next().await {
                yield package.trim_start().to_owned();
            }
        };

        Ok((child, Box::pin(stream)))
    }

    pub async fn status(mut self) -> io::Result<()> {
        self.0.status().await?.into_result()
    }

    pub async fn spawn_with_stdout(self) -> io::Result<(Child, ChildStdout)> {
        crate::utils::spawn_with_stdout(self.0).await
    }
}

pub fn predepends(
    mut lines: impl Stream<Item = String> + Unpin,
    predepend: String,
) -> impl Stream<Item = String> {
    async_stream::stream! {
        let mut active = match lines.next().await {
            Some(line) => line,
            None => return,
        };

        let mut found = false;

        while let Some(line) = lines.next().await {
            if !line.starts_with(' ') {
                let prev = active;
                active = line.trim().to_owned();
                if found {
                    yield prev;
                    found = false
                }
            } else if !found && line.starts_with("  PreDepends: ") && &line[14..] == predepend
            {
                found = true;
            }
        }
    }
}
