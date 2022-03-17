use std::path::PathBuf;

use futures::StreamExt;
use tokio::process::Child;
use tokio::process::Command;

use crate::remote::build::build_test;

pub struct Output {
    child: Child,
}

pub async fn run(test: &str, args: &[&str], envs: Vec<(&str, &str)>) -> Output {
    let root: PathBuf = std::env::var_os("CARGO_MANIFEST_DIR").unwrap().into();
    build_test(root.clone(), test).await;

    let bin_path = root.join("..").join("target").join("release").join(test);
    let mut cmd = Command::new(bin_path);
    let child = cmd
        .current_dir(root.join(".."))
        .envs(envs)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    Output { child }
}

pub async fn kill(test: &str) {
    let mut cmd = Command::new("killall");
    cmd.arg(test);
    cmd.output().await.unwrap();
}

impl Output {
    pub fn output_stream(&mut self) -> futures::stream::BoxStream<'static, String> {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let stdout = self.child.stdout.take().unwrap();
        let stderr = self.child.stderr.take().unwrap();
        let stdout = tokio_stream::wrappers::LinesStream::new(BufReader::new(stdout).lines());
        let stderr = tokio_stream::wrappers::LinesStream::new(BufReader::new(stderr).lines());
        let stdout = stdout
            .filter_map(|line| {
                futures::future::ready(line.map(|line| format!("[STDOUT] {}", line)).ok())
            })
            .boxed();
        let stderr = stderr
            .filter_map(|line| {
                futures::future::ready(line.map(|line| format!("[STDERR] {}", line)).ok())
            })
            .boxed();
        futures::stream::select_all(vec![stdout, stderr]).boxed()
    }
}
