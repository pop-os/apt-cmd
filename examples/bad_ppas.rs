use apt_cmd::AptGet;
use futures::stream::StreamExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let stream = AptGet::new().noninteractive().stream_update().await?;
    futures::pin_mut!(stream);

    while let Some(event) = stream.next().await {
        println!("{:?}", event);
    }

    Ok(())
}
