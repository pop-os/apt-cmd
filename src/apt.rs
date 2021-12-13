// Copyright 2021 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

use anyhow::Context;
use async_process::{Child, Command, Stdio};
use futures::io::BufReader;
use futures::prelude::*;
use futures::stream::{Stream, StreamExt};
use std::pin::Pin;

pub type Packages = Pin<Box<dyn Stream<Item = String>>>;

/// Fetch all upgradeable debian packages from system apt repositories.
pub async fn upgradable_packages() -> anyhow::Result<(Child, Packages)> {
    let mut child = Command::new("apt")
        .args(&["list", "--upgradable"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to launch `apt`")?;

    let stdout = child.stdout.take().unwrap();

    let stream = Box::pin(async_stream::stream! {
        let lines = BufReader::new(stdout).lines().skip(1);

        futures_util::pin_mut!(lines);

        while let Some(Ok(line)) = lines.next().await {
            if let Some(package) = line.split("/").next() {
                yield package.into();
            }
        }
    });

    Ok((child, stream))
}

/// Fetch debian packages which are necessary security updates, only.
pub async fn security_updates() -> anyhow::Result<(Child, Packages)> {
    let mut child = Command::new("apt")
        .args(&["-s", "dist-upgrade"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("could not launch `apt` process")?;

    let stdout = child
        .stdout
        .take()
        .context("`apt` didn't have stdout pipe")?;

    let stream = Box::pin(async_stream::stream! {
        let mut lines = BufReader::new(stdout).lines().skip(1);

        while let Some(Ok(line)) = lines.next().await {
            if let Some(package) = parse_security_update(&line) {
                yield package.into()
            }
        }
    });

    Ok((child, stream))
}

fn parse_security_update(simulated_line: &str) -> Option<&str> {
    if simulated_line.starts_with("Inst") && simulated_line.contains("-security") {
        simulated_line.split_ascii_whitespace().nth(1)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn parse_security_update() {
        assert_eq!(
            Some("libcaca0:i386"),
            super::parse_security_update("Inst libcaca0:i386 [0.99.beta19-2.2ubuntu2] (0.99.beta19-2.2ubuntu2.1 Ubuntu:21.10/impish-security, Ubuntu:21.10/impish-updates [amd64])")
        );

        assert_eq!(
            None,
            super::parse_security_update("Conf libcaca0:i386 [0.99.beta19-2.2ubuntu2] (0.99.beta19-2.2ubuntu2.1 Ubuntu:21.10/impish-security, Ubuntu:21.10/impish-updates [amd64])")
        );
    }
}
