use anyhow::Context;
use async_process::{Child, Command, Stdio};
use futures::io::BufReader;
use futures::prelude::*;
use futures::stream::{Stream, StreamExt};
use std::pin::Pin;

pub type Packages = Pin<Box<dyn Stream<Item = String>>>;

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
