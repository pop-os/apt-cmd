# apt-cmd

Async library for interacting with apt commands from Rust. Key features include:

- Using `apt-get update` to locate PPAs with errors
- Fetching a list of packages to download with `apt-get full-upgrade --print-uris`
- Downloading those packages