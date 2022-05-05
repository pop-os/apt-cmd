#[tokio::main]
async fn main() {
    for package in apt_cmd::apt::downgradable_packages().await.unwrap() {
        println!("{:?}", package);
    }
}
