// Copyright 2021-2022 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

use anyhow::Context;
use as_result::IntoResult;
use std::io;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[derive(AsMut, Deref, DerefMut)]
#[as_mut(forward)]
pub struct AptMark(Command);

impl AptMark {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let mut cmd = Command::new("apt-mark");
        cmd.env("LANG", "C");
        Self(cmd)
    }

    pub async fn hold<I, S>(mut self, packages: I) -> io::Result<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        self.arg("hold");
        self.args(packages);
        self.status().await
    }

    pub async fn unhold<I, S>(mut self, packages: I) -> io::Result<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        self.arg("unhold");
        self.args(packages);
        self.status().await
    }

    /// Shows packages that have been held.
    pub async fn held() -> anyhow::Result<Vec<String>> {
        scrape_packages(AptMark::new().arg("showhold")).await
    }

    /// Obtains a list of automatically-installed packages.
    pub async fn auto_installed() -> anyhow::Result<Vec<String>> {
        scrape_packages(AptMark::new().arg("showauto")).await
    }

    /// Obtains a list of manually-installed packages.
    pub async fn manually_installed() -> anyhow::Result<Vec<String>> {
        scrape_packages(AptMark::new().arg("showmanual")).await
    }

    /// Obtains list of all installed packages.
    pub async fn installed() -> anyhow::Result<Vec<String>> {
        let (mut auto, manual) =
            futures::future::try_join(AptMark::auto_installed(), AptMark::manually_installed())
                .await?;

        auto.extend_from_slice(&manual);
        Ok(auto)
    }

    pub async fn status(mut self) -> io::Result<()> {
        self.0.status().await?.into_result()
    }
}

async fn scrape_packages(command: &mut tokio::process::Command) -> anyhow::Result<Vec<String>> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("failed to spawn `apt-mark showmanual` command")?;

    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let mut packages = Vec::new();
    let mut buffer = String::new();

    loop {
        let read = stdout
            .read_line(&mut buffer)
            .await
            .context("failed to read output of `apt-mark showmanual`")?;

        if read == 0 {
            break;
        }

        packages.push(buffer.trim_end().to_owned());
        buffer.clear();
    }

    let _ = child
        .wait()
        .await
        .context("`apt-mark showmanual` exited with failure status");

    Ok(packages)
}
