use futures::stream::StreamExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (mut child, packages) = apt_cmd::apt::upgradable_packages().await?;

    futures::pin_mut!(packages);

    while let Some(package) = packages.next().await {
        println!("package: {}", package);
    }

    let _ = child.wait().await;

    Ok(())
}
