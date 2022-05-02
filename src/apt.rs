// Copyright 2021-2022 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

use anyhow::Context;
use futures::stream::{Stream, StreamExt};
use std::pin::Pin;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio_stream::wrappers::LinesStream;

pub type Packages = Pin<Box<dyn Stream<Item = String>>>;

/// Locates all packages which do not belong to a repository
pub async fn remoteless_packages() -> anyhow::Result<Vec<String>> {
    let manually_installed = crate::AptMark::new().manually_installed().await?;
    let (mut child, mut stream) = crate::AptCache::new().policy(&manually_installed).await?;

    let mut packages = Vec::new();

    'outer: while let Some(policy) = stream.next().await {
        for sources in policy.version_table.values() {
            for source in sources {
                if !source.contains("/var/lib/dpkg/status") {
                    continue 'outer;
                }
            }
        }

        packages.push(policy.package);
    }

    let _ = child
        .wait()
        .await
        .context("`apt-cache policy` exited in error")?;

    Ok(packages)
}

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
        let mut lines = LinesStream::new(BufReader::new(stdout).lines()).skip(1);

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
        let mut lines = LinesStream::new(BufReader::new(stdout).lines()).skip(1);

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
