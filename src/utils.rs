// Copyright 2021-2022 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

use std::io;
use std::process::Stdio;
use tokio::process::{Child, ChildStdout, Command};

pub async fn spawn_with_stdout(mut command: Command) -> io::Result<(Child, ChildStdout)> {
    command.stdout(Stdio::piped());
    command.stderr(Stdio::inherit());
    command.spawn().map(|mut child| {
        let stdout = child.stdout.take().unwrap();
        (child, stdout)
    })
}
