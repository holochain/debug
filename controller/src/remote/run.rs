use futures::StreamExt;
use openssh as ssh;
use std::net::IpAddr;
use std::path::PathBuf;
use std::str::FromStr;

pub struct Session {
    session: ssh::Session,
    remote_dir: PathBuf,
}

pub struct Output<'session> {
    child: ssh::RemoteChild<'session>,
}

impl<'session> Output<'session> {
    pub fn output_stream(&mut self) -> futures::stream::BoxStream<'static, String> {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let stdout = self.child.stdout().take().unwrap();
        let stderr = self.child.stderr().take().unwrap();
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

impl Session {
    pub async fn new(user: &str, ip: &str, remote_dir: impl Into<PathBuf>) -> Self {
        let session = ssh::Session::connect_mux(
            format!("{}@{}", user, IpAddr::from_str(ip).unwrap()),
            ssh::KnownHosts::Strict,
        )
        .await
        .unwrap();
        Self {
            session,
            remote_dir: remote_dir.into(),
        }
    }

    pub async fn kill(&self, test: &str) {
        self.session.check().await.unwrap();
        let mut cmd = self.session.command("killall");
        cmd.arg(test);
        cmd.output().await.unwrap();
    }

    pub async fn run(&self, test: &str, args: &[&str], envs: &[&str]) -> Output<'_> {
        self.session.check().await.unwrap();
        let mut cmd = self.session.command("cd");
        let a1 = [self.remote_dir.to_str().unwrap(), "&&"];
        let bin_path = format!("./{}", test);
        let a2 = [bin_path.as_str()];
        let a_env = ["env"];
        if envs.is_empty() {
            cmd.raw_args(a1.iter().chain(a2.iter()).chain(args.iter()))
                .stdout(ssh::process::Stdio::piped())
                .stderr(ssh::process::Stdio::piped());
            let child = cmd.spawn().await.unwrap();
            Output { child }
        } else {
            cmd.raw_args(
                a1.iter()
                    .chain(a_env.iter())
                    .chain(envs.iter())
                    .chain(a2.iter())
                    .chain(args.iter()),
            )
            .stdout(ssh::process::Stdio::piped())
            .stderr(ssh::process::Stdio::piped());
            let child = cmd.spawn().await.unwrap();
            Output { child }
        }
    }
}
