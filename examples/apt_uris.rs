use apt_cmd::AptGet;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    for package in AptGet::new()
        .noninteractive()
        .fetch_uris(&["full-upgrade"])
        .await??
    {
        println!("{:?}", package);
    }

    Ok(())
}
