use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use holochain_state::prelude::DatabaseResult;
use holochain_state::prelude::DbKindDht;
use mock_network::types::*;
use mock_network::*;
use observability::tracing::*;
use sessions::prelude::*;

struct MyData(Vec<u8>);

impl<'a> arbitrary::Arbitrary<'a> for MyData {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self(u.bytes(400)?.to_vec()))
    }
}

#[tokio::main]
async fn main() {
    // let _ = observability::test_run_console();
    let _ = observability::test_run();

    const NUM_CONDUCTORS: usize = 10;
    const INLINE: bool = false;

    let entry_def = EntryDef::default_with_id("entrydef");

    let dna_file = if INLINE {
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
        dna_file
    } else {
        let dna_path = sessions::build::build_dna(
            &Path::new("/home/freesig/holochain/debug/test_app/test_zome/Cargo.toml"),
            "test_app",
        )
        .await;

        let dna_file = SweetDnaFile::from_bundle(&dna_path).await.unwrap();
        dna_file
    };

    let mut network = KitsuneP2pConfig::default();
    let mut tuning =
        kitsune_p2p_types::config::tuning_params_struct::KitsuneP2pTuningParams::default();
    tuning.gossip_outbound_target_mbps = 10.0;
    tuning.gossip_inbound_target_mbps = 10.0;
    tuning.gossip_historic_outbound_target_mbps = 10.0;
    tuning.gossip_historic_inbound_target_mbps = 10.0;
    network.tuning_params = std::sync::Arc::new(tuning);
    network.transport_pool = vec![kitsune_p2p::TransportConfig::Quic {
        bind_to: None,
        override_host: None,
        override_port: None,
    }];
    let config = ConductorConfig {
        network: Some(network),
        ..Default::default()
    };
    let mut conductors = Vec::with_capacity(NUM_CONDUCTORS);
    for _ in 0..NUM_CONDUCTORS {
        let envs = test_environments_in("/tmp/");
        let handle = SweetConductor::handle_from_existing(&envs, &config, &[]).await;
        let conductor = SweetConductor::new(handle, envs, config.clone()).await;
        conductors.push(conductor);
    }
    // let mut conductors = SweetConductorBatch::from_config(NUM_CONDUCTORS, config).await;
    let mut conductors = SweetConductorBatch::from(conductors);

    let dna_hash = dna_file.dna_hash().clone();

    let mut existing_db = std::env::var_os("TEST_DB").map(|s| PathBuf::from(s));

    if existing_db.is_none() {
        let s = std::time::Instant::now();
        let keystore = holochain_keystore::test_keystore::spawn_test_keystore()
            .await
            .unwrap();

        let agent_data = (0..5000)
            .map(|_| {
                let data = std::iter::repeat_with(|| {
                    let my_data: MyData = types::make(|u| u.arbitrary());
                    let entry = Entry::App(
                        SerializedBytes::from(UnsafeBytes::from(my_data.0))
                            .try_into()
                            .unwrap(),
                    );
                    let entry_hash = EntryHash::with_data_sync(&entry);
                    let entry = Some(entry);
                    let header = CreateBuilder {
                        entry_type: Some(EntryType::App(AppEntryType::new(
                            0.into(),
                            0.into(),
                            EntryVisibility::Private,
                        ))),
                        entry_hash: Some(entry_hash),
                        ..Default::default()
                    };
                    ChainData {
                        header: header.into(),
                        entry,
                    }
                })
                .take(70);

                Generate {
                    keystore: keystore.clone(),
                    data,
                    dna_hash: dna_hash.clone(),
                    genesis_settings: GenesisBuilder::default(),
                }
            })
            .collect();
        // let agent_data = vec![agent_data];

        let data = GenerateBatch { agent_data };

        let data = data.make().await;

        let bytes: usize = data
            .values()
            .flatten()
            .map(|el| {
                el.entry().as_option().map_or(0, |e| match e {
                    Entry::App(b) => b.bytes().len(),
                    _ => 0,
                })
            })
            .sum();

        let mb = (bytes / 1024) / 1024;
        warn!(generate_data_in = ?s.elapsed(), %mb, %bytes);
        let s = std::time::Instant::now();

        let env = test_in_mem_db(DbKindDht(Arc::new(dna_hash.clone())));

        env.async_commit(move |txn| {
            for element in data.values().flatten() {
                insert_element_as_authority(txn, element)
            }
            DatabaseResult::Ok(data)
        })
        .await
        .unwrap();

        warn!(data_inserted_in = ?s.elapsed());
        let s = std::time::Instant::now();

        let path = dump_database(&env, "");
        warn!(data_dumped_in = ?s.elapsed());
        existing_db = Some(path);
    }

    for c in conductors.iter() {
        if let Some(db) = &existing_db {
            let root_path = c.envs().path().to_path_buf();
            let path = root_path.join("dht");
            std::fs::create_dir(&path).unwrap();
            let path = path.join(format!("dht-{}.sqlite3", dna_hash));
            let bytes = std::fs::copy(db, &path).unwrap();
            warn!(
                "Copied {} MB from {} to {} database.",
                bytes,
                db.display(),
                path.display()
            );
        }
    }

    let apps = conductors
        .setup_app("app", &[dna_file.clone()])
        .await
        .unwrap();

    let cells = apps.cells_flattened();

    let s = std::time::Instant::now();

    let mut headers = Vec::new();
    for _ in 0..100 {
        let s = std::time::Instant::now();
        let h: HeaderHash = if INLINE {
            conductors[0]
                .call(&cells[0].zome("zome1"), "create", ())
                .await
        } else {
            conductors[0]
                .call(&cells[0].zome("test_zome"), "create", ())
                .await
        };
        dbg!(s.elapsed());
        headers.push(h)
    }

    warn!(calls_made_in = ?s.elapsed());

    conductors.exchange_peer_info().await;
    for (i, c) in cells.iter().enumerate().skip(1) {
        for (j, h) in headers.iter().enumerate() {
            let mut cascade = Cascade::empty().with_dht(c.dht_env().clone().into());
            while let None = cascade
                .dht_get(h.clone().into(), Default::default())
                .await
                .unwrap()
            {
                warn!(
                    "Failed to get header {} for cell {} after {:?}",
                    j,
                    i,
                    s.elapsed()
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }

    // loop {
    //     let s = std::time::Instant::now();
    //     let h: HeaderHash = conductors[0]
    //         .call(&cells[0].zome("zome1"), "create", ())
    //         .await;
    //     dbg!(s.elapsed());
    //     for (i, c) in conductors.iter().enumerate().skip(1) {
    //         while let None = c
    //             .call::<_, Option<Element>, _>(&cells[i].zome("zome1"), "read", h.clone())
    //             .await
    //         {
    //             warn!(
    //                 "Failed to get header for cell {} after {:?}",
    //                 i,
    //                 s.elapsed()
    //             );
    //             tokio::time::sleep(Duration::from_millis(100)).await;
    //         }
    //     }
    // }
    tokio::time::sleep(Duration::from_secs(60 * 60)).await;
}
