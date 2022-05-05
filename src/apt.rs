// Copyright 2021-2022 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

use anyhow::Context;
use futures::stream::{Stream, StreamExt};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::pin::Pin;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio_stream::wrappers::LinesStream;

pub type Packages = Pin<Box<dyn Stream<Item = String>>>;

/// It is orphaned if the only source is `/var/lib/dpkg/status`.
fn is_orphaned_version(sources: &[String]) -> bool {
    sources.len() == 1 && sources[0].contains("/var/lib/dpkg/status")
}

/// The version of the package installed which has no repository.
fn orphaned_version(version_table: &HashMap<String, Vec<String>>) -> Option<&str> {
    for (status, sources) in version_table {
        if is_orphaned_version(sources) {
            return Some(status.as_str());
        }
    }

    None
}

/// A list of package versions associated with a repository.
fn repository_versions(version_table: &HashMap<String, Vec<String>>) -> impl Iterator<Item = &str> {
    version_table.iter().filter_map(|(version, sources)| {
        if is_orphaned_version(sources) {
            None
        } else {
            Some(version.as_str())
        }
    })
}

fn greatest_repository_version(version_table: &HashMap<String, Vec<String>>) -> Option<&str> {
    let mut iterator = repository_versions(version_table);
    if let Some(mut greatest_nonlocal) = iterator.next() {
        for nonlocal in iterator {
            if let Ordering::Less = deb_version::compare_versions(greatest_nonlocal, nonlocal) {
                greatest_nonlocal = nonlocal;
            }
        }

        return Some(greatest_nonlocal);
    }

    None
}

// Locates packages which can be downgraded.
pub async fn downgradable_packages() -> anyhow::Result<Vec<(String, String)>> {
    let installed = crate::AptMark::installed().await?;
    let (mut child, mut stream) = crate::AptCache::new().policy(&installed).await?;

    let mut packages = Vec::new();

    'outer: while let Some(policy) = stream.next().await {
        if let Some(local) = orphaned_version(&policy.version_table) {
            if let Some(nonlocal) = greatest_repository_version(&policy.version_table) {
                if let Ordering::Greater = deb_version::compare_versions(local, nonlocal) {
                    packages.push((policy.package, nonlocal.to_owned()));
                    continue 'outer;
                }
            }
        }
    }

    let _ = child
        .wait()
        .await
        .context("`apt-cache policy` exited in error")?;

    Ok(packages)
}

/// Locates all packages which do not belong to a repository
pub async fn remoteless_packages() -> anyhow::Result<Vec<String>> {
    let installed = crate::AptMark::installed().await?;
    let (mut child, mut stream) = crate::AptCache::new().policy(&installed).await?;

    let mut packages = Vec::new();

    'outer: while let Some(policy) = stream.next().await {
        for sources in policy.version_table.values() {
            if !is_orphaned_version(sources) {
                continue 'outer;
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
