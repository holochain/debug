use std::sync::Arc;

use mock_network::agent_info::GenerateAgentInfo;
use mock_network::agent_info::GenerateQuicAgents;
use mock_network::types::*;
use mock_network::*;
use sessions::prelude::*;

#[tokio::main]
async fn main() {
    const NUM_AGENTS: usize = 5000;

    let entry_def = EntryDef::default_with_id("entrydef");
    let zome = InlineZome::new_unique(vec![entry_def.clone()])
        .callback("create", move |api, ()| {
            let entry_def_id: EntryDefId = entry_def.id.clone();
            let entry = Entry::app(().try_into().unwrap()).unwrap();
            let hash = api.create(CreateInput::new(
                entry_def_id,
                entry,
                ChainTopOrdering::default(),
            ))?;
            Ok(hash)
        })
        .callback("read", |api, hash: HeaderHash| {
            api.get(vec![GetInput::new(hash.into(), GetOptions::default())])
                .map(|e| e.into_iter().next().unwrap())
                .map_err(Into::into)
        });
    let (dna_file, _) = SweetDnaFile::from_inline_zome("".into(), "zome1", zome)
        .await
        .unwrap();
    let dna_hash = dna_file.dna_hash().clone();
    let keystore = holochain_keystore::test_keystore::spawn_test_keystore()
        .await
        .unwrap();
    let agent_data = (0..NUM_AGENTS)
        .map(|_| Generate {
            keystore: keystore.clone(),
            data: vec![],
            dna_hash: dna_hash.clone(),
            genesis_settings: GenesisBuilder::default(),
        })
        .collect();

    let data: GenerateBatch<Vec<ChainData>> = GenerateBatch { agent_data };

    let data = data.make().await;

    let settings = mock_network::agent_info::SettingsBuilder::default();
    let agents = GenerateAgentInfo {
        keystore: &keystore,
        agent_keys: data.keys(),
        dna_hash: dna_hash.clone(),
        settings,
    };
    let (agent_info, quic) = GenerateQuicAgents { agents }.make().await;

    let agent_info = Arc::new(agent_info);
    let mut channel = mock_network::real_networked(agent_info.clone(), quic);

    let mut conductor = SweetConductor::from_standard_config().await;
    let apps = conductor.setup_app("app", &vec![dna_file]).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    let infos = conductor
        .get_agent_infos(Some(apps.cells()[0].cell_id().clone()))
        .await
        .unwrap();
    dbg!(&infos);
    channel.add_real_agents(infos);
    dbg!();
    conductor
        .add_agent_infos((*agent_info).clone())
        .await
        .unwrap();
    dbg!();

    while let Some((msg, respond)) = channel.next().await {
        dbg!(&msg.from_agent);
        dbg!(&msg.to_agent);
        channel.default_response(msg, respond).await;
    }
}
