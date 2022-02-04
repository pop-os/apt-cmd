// Copyright 2021-2022 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

use as_result::IntoResult;
use std::io;
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

    pub async fn status(mut self) -> io::Result<()> {
        self.0.status().await?.into_result()
    }
}
