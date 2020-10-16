use apt_cmd::AptGet;
use futures_lite::stream::StreamExt;
use futures_util::pin_mut;

fn main() -> anyhow::Result<()> {
    futures_lite::future::block_on(async move {
        let stream = AptGet::new().noninteractive().update().await?;
        pin_mut!(stream);

        while let Some(event) = stream.next().await {
            println!("{:?}", event);
        }

        Ok(())
    })
}
