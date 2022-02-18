use holochain_cli::hc_bundle::HcDnaBundle;
use std::path::{Path, PathBuf};

pub async fn build_dna(dna: &Path) -> PathBuf {
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

    let dna_path = workdir.join(&format!("{}.dna", dna));
    let bundle = HcDnaBundle::Pack {
        path: workdir,
        output: Some(dna_path.clone()),
    };
    bundle.run().await.unwrap();
    dna_path
}
