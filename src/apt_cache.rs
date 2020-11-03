use anyhow::Context;
use as_result::{IntoResult, MapResult};
use async_process::{Child, ChildStdout, Command};
use futures::io::BufReader;
use futures::prelude::*;
use futures::stream::{Stream, StreamExt};
use std::io;
use std::pin::Pin;

pub type PackageStream = Pin<Box<dyn Stream<Item = String>>>;

#[derive(Debug, Clone)]
pub struct Policy {
    pub package: String,
    pub installed: String,
    pub candidate: String,
}

pub type Policies = Pin<Box<dyn Stream<Item = Policy>>>;

fn policies(lines: impl Stream<Item = io::Result<String>>) -> impl Stream<Item = Policy> {
    async_stream::stream! {
        futures_util::pin_mut!(lines);

        let mut policy = Policy {
            package: String::new(),
            installed: String::new(),
            candidate: String::new()
        };

        while let Some(Ok(line)) = lines.next().await {
            if line.is_empty() {
                continue
            }

            if !line.starts_with(' ') {
                policy.package = line;
                continue
            }

            let line = line.trim();

            if line.starts_with("I") {
                if let Some(v) = line.split_ascii_whitespace().nth(1) {
                    policy.installed = v.to_owned();
                }
            } else if line.starts_with("C") {
                if let Some(v) = line.split_ascii_whitespace().nth(1) {
                    policy.candidate = v.to_owned();
                }

                yield policy.clone();
            }
        }
    }
}

#[derive(AsMut, Deref, DerefMut)]
#[as_mut(forward)]
pub struct AptCache(Command);

impl AptCache {
    pub fn new() -> Self {
        let mut cmd = Command::new("apt-cache");
        cmd.env("LANG", "C");
        Self(cmd)
    }

    pub async fn depends<I, S>(mut self, packages: I) -> io::Result<(Child, ChildStdout)>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        self.arg("depends");
        self.args(packages);
        self.spawn_with_stdout().await
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

    pub async fn policy<'a>(
        mut self,
        packages: &'a [&'a str],
    ) -> anyhow::Result<(Child, Policies)> {
        self.arg("policy");
        self.args(packages);
        self.env("LANG", "C");

        let (child, stdout) = self.spawn_with_stdout().await?;

        let stream = Box::pin(policies(BufReader::new(stdout).lines()));

        Ok((child, stream))
    }

    pub async fn predepends_of<'a>(
        out: &'a mut String,
        package: &'a str,
    ) -> anyhow::Result<Vec<&'a str>> {
        let (mut child, mut packages) = AptCache::new()
            .rdepends(&[&package])
            .await
            .with_context(|| format!("failed to launch `apt-cache rdepends {}`", package))?;

        let mut depends = Vec::new();
        while let Some(package) = packages.next().await {
            depends.push(package);
        }

        child
            .status()
            .await
            .map_result()
            .with_context(|| format!("bad status from `apt-cache rdepends {}`", package))?;

        let (mut child, mut stdout) = AptCache::new()
            .depends(&depends)
            .await
            .with_context(|| format!("failed to launch `apt-cache depends {}`", package))?;

        stdout
            .read_to_string(out)
            .await
            .with_context(|| format!("failed to get output of `apt-cache depends {}`", package))?;

        child.status().await.map_result()?;

        Ok(PreDependsIter::new(out.as_str(), package)?.collect::<Vec<_>>())
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
pub struct PreDependsIter<'a> {
    lines: std::str::Lines<'a>,
    predepend: &'a str,
    active: &'a str,
}

impl<'a> PreDependsIter<'a> {
    pub fn new(output: &'a str, predepend: &'a str) -> io::Result<Self> {
        let mut lines = output.lines();

        let active = lines.next().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "expected the first line of the output of apt-cache depends to be a package name",
            )
        })?;

        Ok(Self {
            lines,
            predepend,
            active: active.trim(),
        })
    }
}

impl<'a> Iterator for PreDependsIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        let mut found = false;
        while let Some(line) = self.lines.next() {
            if !line.starts_with(' ') {
                let prev = self.active;
                self.active = line.trim();
                if found {
                    return Some(prev);
                }
            } else if !found && line.starts_with("  PreDepends: ") && &line[14..] == self.predepend
            {
                found = true;
            }
        }

        None
    }
}
