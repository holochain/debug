use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use hdrhistogram::Histogram;
use holochain_state::prelude::DatabaseResult;
use holochain_types::prelude::DbKindDht;
use kitsune_p2p::dht_arc::DhtArc;
use kitsune_p2p_types::Tx2Cert;
use mock_network::agent_info::GenerateAgentInfo;
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

    const INLINE: bool = false;
    const NUM_AGENTS: usize = 5000;

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

    let dna_hash = dna_file.dna_hash().clone();

    let s = std::time::Instant::now();
    let keystore = holochain_keystore::test_keystore::spawn_test_keystore()
        .await
        .unwrap();

    let agent_data = (0..NUM_AGENTS)
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

    let data = GenerateBatch { agent_data };

    let data = data.make().await;

    let mut settings = mock_network::agent_info::SettingsBuilder::default();
    let ideal_length =
        mock_network::agent_info::ideal_target(NUM_AGENTS + 1, DhtArc::full(0u32.into()))
            .coverage();
    settings.dht_storage_arc_coverage(ideal_length);
    let agent_info = GenerateAgentInfo {
        keystore: &keystore,
        agent_keys: data.keys(),
        dna_hash: dna_hash.clone(),
        settings,
    }
    .make()
    .await;

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

    let (mut channel, mut network) = mock_network::setup(agent_info.clone());
    let mut tuning =
        kitsune_p2p_types::config::tuning_params_struct::KitsuneP2pTuningParams::default();
    tuning.gossip_outbound_target_mbps = 10.0;
    tuning.gossip_inbound_target_mbps = 10.0;
    tuning.gossip_historic_outbound_target_mbps = 10.0;
    tuning.gossip_historic_inbound_target_mbps = 10.0;
    let tuning_params = std::sync::Arc::new(tuning);
    network.tuning_params = tuning_params.clone();
    let config = ConductorConfig {
        network: Some(network),
        ..Default::default()
    };

    let envs = test_dbs_in("/tmp/");
    let keystore = envs.keystore().clone();
    let handle =
        SweetConductor::handle_from_existing(envs.path(), keystore.clone(), &config, &[]).await;
    let mut conductor = SweetConductor::new(handle, envs.into_tempdir(), config.clone()).await;

    let alice = SweetAgents::one(keystore).await;
    let alice_arc = DhtArc::with_coverage(alice.get_loc(), ideal_length);

    conductor
        .add_agent_infos(
            agent_info
                .iter()
                .filter(|a| alice_arc.contains(a.storage_arc.start_loc()))
                .cloned()
                .collect(),
        )
        .await
        .unwrap();

    let agent_info = Arc::new(agent_info);
    let gen_data = mock_network::GeneratedData::new(data.clone(), agent_info.clone()).await;
    let gossip_data_requests = mock_network::DataRequestsBuilder::default().data(gen_data.clone());
    // let gossip_data_requests = mock_network::DataRequestsBuilder::default();

    let gossip_data = mock_network::Gossip::new(
        tuning_params,
        &dna_hash,
        data.keys().cloned(),
        gossip_data_requests,
    )
    .await;

    dbg!();

    let s = std::time::Instant::now();

    let env = test_in_mem_db(DbKindDht(Arc::new(dna_hash.clone())));

    let ops_alice_holds: HashSet<_> = gen_data
        .agent_loc_time_idx
        .iter()
        .filter(|((_, l, _), _)| alice_arc.contains(*l))
        .flat_map(|(_, h)| h.clone())
        .collect();

    let gd = gen_data.clone();
    env.async_commit(move |txn| {
        for (khash, (_, element)) in gd.agent_khash_idx.values().flatten() {
            if ops_alice_holds.contains(khash.as_ref()) {
                insert_element_as_authority(txn, element);
            }
        }
        DatabaseResult::Ok(())
    })
    .await
    .unwrap();

    warn!(data_inserted_in = ?s.elapsed());
    let s = std::time::Instant::now();

    let db = dump_database(&env, "");
    warn!(data_dumped_in = ?s.elapsed());

    let root_path = conductor.db_path().to_path_buf();
    let path = root_path.join("dht");
    std::fs::create_dir(&path).unwrap();
    let path = path.join(format!("dht-{}.sqlite3", dna_hash));
    let bytes = std::fs::copy(&db, &path).unwrap();
    warn!(
        "Copied {} MB from {} to {} database.",
        bytes,
        db.display(),
        path.display()
    );

    let app = conductor
        .setup_app_for_agent("app", alice, &[dna_file.clone()])
        .await
        .unwrap();

    let cell = app.into_cells();

    let env = cell[0].dht_db();

    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    let ai = conductor
        .get_agent_infos(Some(cell[0].cell_id().clone()))
        .await
        .unwrap();
    eprintln!(
        "Alice {:.2} = {:.2}",
        alice_arc.coverage(),
        ai[0].storage_arc.coverage()
    );

    tokio::spawn({
        let gen_data = gen_data.clone();
        async move {
            let mut hits_hist = Histogram::<u64>::new(3).unwrap();
            let mut miss_hist = Histogram::<u64>::new(3).unwrap();

            loop {
                for hash in gen_data.data.values().flat_map(|v| {
                    v.iter()
                        .map(|el| el.header_address())
                        .cloned()
                        .collect::<Vec<_>>()
                }) {
                    let s = std::time::Instant::now();
                    let r: Option<Element> = if INLINE {
                        conductor.call(&cell[0].zome("zome1"), "read", hash).await
                    } else {
                        conductor
                            .call(&cell[0].zome("test_zome"), "read", hash)
                            .await
                    };
                    let elapsed = s.elapsed();
                    if r.is_some() {
                        hits_hist.record(elapsed.as_micros() as u64).unwrap();
                    } else {
                        miss_hist.record(elapsed.as_micros() as u64).unwrap();
                    }

                    let hist = hits_hist.iter_quantiles(1).fold(
                        (String::new(), Duration::default()),
                        |(mut out, mut last), i| {
                            let e = Duration::from_micros(i.value_iterated_to());
                            out = format!("{}{:?}->{:?}: {} \n", out, last, e, i.count_at_value());
                            last = e;
                            (out, last)
                        },
                    );
                    eprintln!("Hits {}", hist.0);
                    let hist = miss_hist.iter_quantiles(1).fold(
                        (String::new(), Duration::default()),
                        |(mut out, mut last), i| {
                            let e = Duration::from_micros(i.value_iterated_to());
                            out = format!("{}{:?}->{:?}: {}\n", out, last, e, i.count_at_value());
                            last = e;
                            (out, last)
                        },
                    );
                    eprintln!("Misses {}", hist.0);

                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    });

    while let Some((msg, respond)) = channel.next().await {
        // eprintln!("{}", holochain::test_utils::query_integration(env).await);
        let to_agent = msg.to_agent.as_key().unwrap();
        let from_agent = msg.from_agent.clone();
        match msg.msg {
            HolochainP2pMockMsg::Wire {
                msg: WireMessage::Get { dht_hash, .. },
                ..
            } => {
                let r = gen_data.get((**to_agent).clone(), dht_hash);
                let r: Vec<u8> = UnsafeBytes::from(SerializedBytes::try_from(r).unwrap()).into();
                let msg = HolochainP2pMockMsg::CallResp(r.into());
                respond.unwrap().respond(msg);
            }
            // HolochainP2pMockMsg::CallResp(_) => todo!(),
            HolochainP2pMockMsg::Gossip {
                gossip,
                module,
                dna,
            } => {
                let gossip = match gossip {
                    GossipProtocol::Sharded(g) => g,
                    _ => todo!(),
                };
                let from = Tx2Cert::from(vec![0u8; 32]);
                let out = gossip_data
                    .process_msg(from, &to_agent, module, gossip)
                    .await;
                for msg in out {
                    let msg = HolochainP2pMockMsg::Gossip {
                        gossip: GossipProtocol::Sharded(msg),
                        module,
                        dna: dna.clone(),
                    };
                    channel
                        .send(msg.addressed(to_agent.clone(), from_agent.clone()))
                        .await;
                }
            }
            HolochainP2pMockMsg::PeerGet(kwire::PeerGet { agent: request, .. }) => {
                let info = gen_data
                    .peer_get(&to_agent, request, &HashMap::new())
                    .unwrap();
                respond
                    .unwrap()
                    .respond(HolochainP2pMockMsg::PeerGetResp(kwire::PeerGetResp {
                        agent_info_signed: info,
                    }));
            }
            HolochainP2pMockMsg::PeerQuery(kwire::PeerQuery { basis_loc, .. }) => {
                let info = gen_data.peer_query(&to_agent, basis_loc);
                respond.unwrap().respond(HolochainP2pMockMsg::PeerQueryResp(
                    kwire::PeerQueryResp { peer_list: info },
                ));
            }
            // HolochainP2pMockMsg::PublishedAgentInfo {
            //     to_agent,
            //     dna,
            //     info,
            // } => todo!(),
            _ => channel.default_response(msg, respond).await,
        }
    }

    // let s = std::time::Instant::now();

    // let mut headers = Vec::new();
    // for _ in 0..100 {
    //     let s = std::time::Instant::now();
    //     let h: HeaderHash = if INLINE {
    //         conductor
    //             .call(&cells[0].zome("zome1"), "create", ())
    //             .await
    //     } else {
    //         conductor
    //             .call(&cells[0].zome("test_zome"), "create", ())
    //             .await
    //     };
    //     dbg!(s.elapsed());
    //     headers.push(h)
    // }

    // warn!(calls_made_in = ?s.elapsed());

    // for (i, c) in cells.iter().enumerate().skip(1) {
    //     for (j, h) in headers.iter().enumerate() {
    //         let mut cascade = Cascade::empty().with_dht(c.dht_env().clone().into());
    //         while let None = cascade
    //             .dht_get(h.clone().into(), Default::default())
    //             .await
    //             .unwrap()
    //         {
    //             warn!(
    //                 "Failed to get header {} for cell {} after {:?}",
    //                 j,
    //                 i,
    //                 s.elapsed()
    //             );
    //             tokio::time::sleep(Duration::from_millis(100)).await;
    //         }
    //     }
    // }

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
