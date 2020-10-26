use apt_cmd::AptGet;
use futures::stream::StreamExt;
use futures_util::pin_mut;

fn main() -> anyhow::Result<()> {
    futures::executor::block_on(async move {
        let stream = AptGet::new().noninteractive().stream_update().await?;
        pin_mut!(stream);

        while let Some(event) = stream.next().await {
            println!("{:?}", event);
        }

        Ok(())
    })
}
