use apt_cmd::AptGet;

fn main() -> anyhow::Result<()> {
    futures::executor::block_on(async move {
        for package in AptGet::new()
            .noninteractive()
            .fetch_uris(&["full-upgrade"])
            .await??
        {
            println!("{:?}", package);
        }

        Ok(())
    })
}
