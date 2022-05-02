#[tokio::main]
async fn main() {
    for package in apt_cmd::apt::remoteless_packages().await.unwrap() {
        println!("{}", package);
    }
}
