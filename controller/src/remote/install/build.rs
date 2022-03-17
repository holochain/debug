use holochain_cli::hc_bundle::HcDnaBundle;
use std::path::{Path, PathBuf};
use tokio::process::Command;

pub async fn build_dna(dna: &Path, name: &str) -> PathBuf {
    let path: PathBuf = std::env::var_os("CARGO_MANIFEST_DIR").unwrap().into();
    let path = path.join("..");
    let workdir = path.join("workdir");
    let dna = dna.display().to_string();

    let mut build = tokio::process::Command::new("cargo");
    build
        .env_remove("RUSTFLAGS")
        .args(&[
            "build",
            "--manifest-path",
            &dna,
            "--release",
            "--target",
            "wasm32-unknown-unknown",
        ])
        .kill_on_drop(true)
        .status()
        .await
        .unwrap();

    let dna_path = workdir.join(&format!("{}.dna", name));
    let bundle = HcDnaBundle::Pack {
        path: workdir,
        output: Some(dna_path.clone()),
    };
    bundle.run().await.unwrap();
    dna_path
}

pub async fn build_test(root: PathBuf, test: &str) {
    let path = root.join("..").join("sessions").join("Cargo.toml");
    let mut build = Command::new("cargo");
    let s = build
        .args(&[
            "build",
            "--manifest-path",
            path.to_str().unwrap(),
            "--release",
            "--bin",
            test,
        ])
        .kill_on_drop(true)
        .status()
        .await
        .unwrap();
    assert!(s.success());
}
