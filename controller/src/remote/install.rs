use std::net::IpAddr;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use tokio::process::*;

use self::build::build_test;

pub mod build;

#[derive(Debug, Clone)]
pub struct Scp {
    user: String,
    ip: IpAddr,
}

impl Scp {
    pub fn new(user: &str, ip: &str) -> Self {
        Self {
            user: user.to_string(),
            ip: IpAddr::from_str(ip).unwrap(),
        }
    }

    pub async fn install(&self, local_path: impl AsRef<Path>, remote_path: impl AsRef<Path>) {
        let s = Command::new("scp")
            .arg(local_path.as_ref())
            .arg(&format!(
                "{}@{}:{}",
                self.user,
                self.ip,
                remote_path.as_ref().display()
            ))
            .kill_on_drop(true)
            .output()
            .await
            .unwrap();
        assert!(s.status.success(), "{:?}", s);
    }

    pub async fn install_test(&self, remote_dir: impl AsRef<Path>, test: &str) {
        let root: PathBuf = std::env::var_os("CARGO_MANIFEST_DIR").unwrap().into();
        build_test(root.clone(), test).await;

        self.install(
            &root.join("..").join("target").join("release").join(test),
            remote_dir,
        )
        .await;
    }

    pub async fn install_dna(
        &self,
        remote_dir: impl AsRef<Path>,
        dna_manifest: &str,
        name: &str,
    ) -> PathBuf {
        let dna_path = build::build_dna(dna_manifest.as_ref(), name).await;
        self.install(&dna_path, &remote_dir).await;
        PathBuf::from(remote_dir.as_ref()).join(dna_path.file_name().unwrap())
    }

    pub async fn install_db(
        &self,
        remote_dir: impl AsRef<Path>,
        db_path: impl AsRef<Path>,
    ) -> PathBuf {
        self.install(&db_path, &remote_dir).await;
        PathBuf::from(remote_dir.as_ref()).join(db_path.as_ref().file_name().unwrap())
    }
}
