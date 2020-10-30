use futures::stream::StreamExt;

fn main() -> anyhow::Result<()> {
    futures::executor::block_on(async move {
        let (child, packages) = apt_cmd::apt::upgradable_packages().await?;

        futures_util::pin_mut!(packages);

        while let Some(package) = packages.next().await {
            println!("package: {}", package);
        }

        Ok(())
    })
}
