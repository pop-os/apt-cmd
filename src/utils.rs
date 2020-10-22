use async_process::{Child, ChildStdout, Command, Stdio};
use std::io;

pub async fn spawn_with_stdout(mut command: Command) -> io::Result<(Child, ChildStdout)> {
    command.stdout(Stdio::piped());
    command.stderr(Stdio::null());
    command.spawn().map(|mut child| {
        let stdout = child.stdout.take().unwrap();
        (child, stdout)
    })
}
