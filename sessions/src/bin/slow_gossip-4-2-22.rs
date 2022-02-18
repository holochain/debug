use std::path::Path;

use mock_network::agent_info::GenerateAgentInfo;
use mock_network::types::*;
use mock_network::*;
use observability::tracing::*;
use sessions::prelude::*;

#[derive(Debug, Serialize, Deserialize, SerializedBytes, Clone)]
pub struct Props {
    pub skip_proof: bool,
    pub holo_agent_override: Option<AgentPubKeyB64>,
}

#[tokio::main]
async fn main() {
    let _ = observability::test_run();
    // let dna_path = PathBuf::from("/home/freesig/holochain/debug/dnas/elemental-chat.dna");
    let dna_path = sessions::build::build_dna(&Path::new(
        "/home/freesig/holochain/debug/test_app/test_zome/Cargo.toml",
    ))
    .await;

    let dna_file = SweetDnaFile::from_bundle_with_overrides(
        &dna_path,
        None,
        // Note that we can use our own native `Props` type
        Some(Props {
            skip_proof: true,
            holo_agent_override: None,
        }),
    )
    .await
    .unwrap();
    let s = std::time::Instant::now();
    let data = std::iter::repeat_with(|| CreateBuilder::default().into())
        .take(1000)
        .chain(std::iter::repeat_with(|| {
            CreateLinkBuilder::default().into()
        }))
        .take(1000);

    let keystore = holochain_keystore::test_keystore::spawn_test_keystore()
        .await
        .unwrap();

    let dna_hash = dna_file.dna_hash().clone();
    let agent_data = Generate {
        keystore: &keystore,
        data,
        dna_hash: dna_hash.clone(),
        genesis_settings: GenesisBuilder::default(),
    };
    let agent_data = vec![agent_data];

    let data = GenerateBatch { agent_data };

    let data = data.make().await;

    let agent_info = GenerateAgentInfo {
        keystore: &keystore,
        agent_keys: data.keys(),
        dna_hash: dna_hash.clone(),
        settings: Default::default(),
    }
    .make()
    .await;

    debug!(generate_data_in = ?s.elapsed());

    let (mut channel, network_config) = mock_network::setup(agent_info.clone());

    let jh = tokio::spawn(async move {
        let mut config = ConductorConfig::default();
        config.network = Some(network_config);

        // Add it to the conductor builder.
        let conductor_builder = ConductorBuilder::new()
            .config(config)
            .with_keystore(keystore);
        let mut conductor = SweetConductor::from_builder(conductor_builder).await;
        // Add in all the agent info.
        conductor.add_agent_infos(agent_info.clone()).await.unwrap();
        // Install the real agent alice.
        let apps = conductor
            .setup_app("app", &[dna_file.clone()])
            .await
            .unwrap();

        let (alice,) = apps.into_tuple();

        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    });

    while let Some((msg, respond)) = channel.next().await {
        dbg!(msg);
    }

    jh.await.unwrap();
}
