// Copyright 2021-2022 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

use crate::request::{Request, RequestError};
use crate::AptUpgradeEvent;
use as_result::*;
use async_stream::stream;
use futures::prelude::*;
use std::process::ExitStatus;
use std::{collections::HashSet, io, pin::Pin};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStdout, Command};

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

pub type UpgradeEvents = Pin<Box<dyn Stream<Item = AptUpgradeEvent>>>;

#[derive(AsMut, Deref, DerefMut)]
#[as_mut(forward)]
pub struct AptGet(Command);

impl AptGet {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let mut cmd = Command::new("apt-get");
        cmd.env("LANG", "C");
        Self(cmd)
    }

    pub fn allow_downgrades(mut self) -> Self {
        self.arg("--allow-downgrades");
        self
    }

    pub fn autoremove(mut self) -> Self {
        self.arg("autoremove");
        self
    }

    pub fn dpkg_option(mut self, option: &str) -> Self {
        self.args(&["-o", &["Dpkg::Options::=", option].concat()]);
        self
    }

    pub fn fix_broken(mut self) -> Self {
        self.args(&["install", "-f"]);
        self
    }

    pub fn force(mut self) -> Self {
        self.arg("-y");
        self
    }

    pub fn force_breaks(self) -> Self {
        self.dpkg_option("--force-breaks")
    }

    pub fn force_confdef(self) -> Self {
        self.dpkg_option("--force-confdef")
    }

    pub fn force_conflicts(self) -> Self {
        self.dpkg_option("--force-conflicts")
    }

    pub fn force_confold(self) -> Self {
        self.dpkg_option("--force-confold")
    }

    pub fn force_depends(self) -> Self {
        self.dpkg_option("--force-depends")
    }

    pub fn force_depends_version(self) -> Self {
        self.dpkg_option("--force-depends-version")
    }

    pub fn force_overwrite(self) -> Self {
        self.dpkg_option("--force-overwrite")
    }

    pub async fn install<I, S>(mut self, packages: I) -> io::Result<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        self.arg("install");
        self.args(packages);

        self.status().await
    }

    pub fn mark_auto(mut self) -> Self {
        self.arg("--mark-auto");
        self
    }

    pub fn noninteractive(mut self) -> Self {
        self.env("DEBIAN_FRONTEND", "noninteractive");
        self
    }

    pub async fn update(mut self) -> io::Result<()> {
        self.arg("update");
        self.status().await
    }

    pub fn simulate(mut self) -> Self {
        self.arg("-s");
        self
    }

    pub async fn upgrade(mut self) -> io::Result<()> {
        self.arg("full-upgrade");
        self.status().await
    }

    pub async fn stream_upgrade(mut self) -> io::Result<(Child, UpgradeEvents)> {
        self.args(&["--show-progress", "full-upgrade"]);

        let (child, stdout) = self.spawn_with_stdout().await?;

        let stream = stream! {
            let mut stdout = BufReader::new(stdout).lines();

            while let Ok(Some(line)) = stdout.next_line().await {
                if let Ok(event) = line.parse::<AptUpgradeEvent>() {
                    yield event;
                }
            }
        };

        Ok((child, Box::pin(stream)))
    }

    pub async fn remove<I, S>(mut self, packages: I) -> io::Result<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        self.arg("remove");
        self.args(packages);

        self.status().await
    }

    pub async fn fetch_uris(
        mut self,
        command: &[&str],
    ) -> io::Result<Result<HashSet<Request>, RequestError>> {
        self.arg("--print-uris");
        self.args(command);

        let (mut child, stdout) = self.spawn_with_stdout().await?;

        let mut stdout = BufReader::new(stdout).lines();

        let mut packages = HashSet::new();

        while let Ok(Some(line)) = stdout.next_line().await {
            if !line.starts_with('\'') {
                continue;
            }

            let package = match line.parse::<Request>() {
                Ok(package) => package,
                Err(why) => return Ok(Err(why)),
            };

            packages.insert(package);
        }

        child.wait().await.map_result()?;

        Ok(Ok(packages))
    }

    pub async fn stream_update(mut self) -> io::Result<Pin<Box<dyn Stream<Item = UpdateEvent>>>> {
        self.arg("update");

        let (mut child, stdout) = self.spawn_with_stdout().await?;

        let mut stdout = BufReader::new(stdout).lines();

        let stream = stream! {
            while let Ok(Some(line)) = stdout.next_line().await {
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

            yield UpdateEvent::ExitStatus(child.wait().await);
        };

        Ok(Box::pin(stream))
    }

    pub async fn spawn_with_stdout(self) -> io::Result<(Child, ChildStdout)> {
        crate::utils::spawn_with_stdout(self.0).await
    }

    pub async fn status(mut self) -> io::Result<()> {
        self.0.status().await?.into_result()
    }
}
