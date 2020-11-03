use futures::stream::StreamExt;

fn main() -> anyhow::Result<()> {
    futures::executor::block_on(async move {
        let (_child, policies) = apt_cmd::AptCache::new()
            .policy(&["firefox", "gnome-shell"])
            .await?;

        futures_util::pin_mut!(policies);

        while let Some(policy) = policies.next().await {
            println!("policy: {:#?}", policy);
        }

        Ok(())
    })
}
