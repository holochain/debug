use futures::StreamExt;
use holochain_types::prelude::p2p_put;
use holochain_types::prelude::p2p_put_all;
use kitsune_p2p::dependencies::url2::Url2;
use mock_network::agent_info::GenerateAgentInfo;
use sessions::prelude::*;
use sessions::shared_types;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clap::ArgEnum;
use clap::Parser;
use hdrhistogram::Histogram;
use kitsune_p2p::dht_arc::DhtArc;

#[derive(Debug, ArgEnum, Clone, Copy)]
enum DnaType {
    Blank,
    EChat,
}

#[derive(Debug, Parser)]
struct Input {
    #[clap(long, default_value_t = 1.0)]
    ideal_length: f64,
    #[clap(long)]
    bootstrap: String,
    #[clap(long)]
    inline_zome: bool,
    #[clap(long)]
    dna_path: PathBuf,
    #[clap(long, arg_enum)]
    dna_type: DnaType,
    #[clap(long, default_value_t = 0)]
    num_other_agents: usize,
    #[clap(long)]
    signals: bool,
}

#[tokio::main]
async fn main() {
    dbg!();
    let Input {
        ideal_length,
        bootstrap,
        inline_zome,
        dna_path,
        dna_type,
        num_other_agents,
        signals,
    } = Input::parse();
    dbg!(&dna_path);
    dbg!(&inline_zome);
    dbg!(&bootstrap);
    dbg!(&ideal_length);
    dbg!(&dna_type);
    dbg!(&num_other_agents);
    let _ = observability::test_run();

    let entry_def = EntryDef::default_with_id("entrydef");
    dbg!();

    let dna_file = if inline_zome {
        let zome = InlineZome::new("".to_string(), vec![entry_def.clone()])
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
        let dna_file = SweetDnaFile::from_bundle(&dna_path).await.unwrap();
        dna_file
    };
    dbg!(&dna_file.dna_hash());

    let mut tuning =
        kitsune_p2p_types::config::tuning_params_struct::KitsuneP2pTuningParams::default();
    tuning.gossip_outbound_target_mbps = 10.0;
    tuning.gossip_inbound_target_mbps = 10.0;
    tuning.gossip_historic_outbound_target_mbps = 10.0;
    tuning.gossip_historic_inbound_target_mbps = 10.0;
    tuning.default_rpc_multi_remote_agent_count = 1;
    tuning.gossip_single_storage_arc_per_space = true;
    let tuning_params = std::sync::Arc::new(tuning);
    let mut network = KitsuneP2pConfig::default();
    network.tuning_params = tuning_params.clone();
    network.bootstrap_service = Some(Url2::try_parse(bootstrap).unwrap());
    network.transport_pool = vec![kitsune_p2p::TransportConfig::Quic {
        bind_to: None,
        override_host: None,
        override_port: None,
    }];
    let config = ConductorConfig {
        network: Some(network),
        ..Default::default()
    };

    let envs = test_dbs_in("/tmp/");
    let keystore = envs.keystore().clone();
    let handle =
        SweetConductor::handle_from_existing(envs.path(), keystore.clone(), &config, &[]).await;
    let mut conductor = SweetConductor::new(handle, envs.into_tempdir(), config.clone()).await;

    let alice = SweetAgents::one(keystore.clone()).await;
    let alice_arc = DhtArc::with_coverage(alice.get_loc(), ideal_length);

    // Insert Peer info to ideal size.
    let mut settings = mock_network::agent_info::SettingsBuilder::default();
    settings.dht_storage_arc_coverage(ideal_length);
    settings.signed_at(std::time::SystemTime::now() - std::time::Duration::from_secs(60 * 60));
    settings.expires_at(std::time::SystemTime::now() + std::time::Duration::from_secs(60));
    let key = Arc::new(alice.clone());
    let dna_hash = dna_file.dna_hash().clone();
    let agents = GenerateAgentInfo {
        keystore: &keystore,
        agent_keys: vec![&key],
        dna_hash: dna_hash.clone(),
        settings,
    };
    let info = agents.make().await.into_iter().next().unwrap();
    dbg!(info.storage_arc.coverage());
    let p2p_store = conductor.get_p2p_db(&dna_hash);
    p2p_put(&p2p_store, &info).await.unwrap();

    let (tx, mut rx, url) = controller::connect::listen::<shared_types::Msg>().await;
    println!("[[URL]] {}", url);
    let mut tx = tx.await.unwrap();
    let msg: shared_types::Msg = tx
        .request(shared_types::Msg::DhtArc(alice_arc))
        .await
        .unwrap();
    let dna_hash = dna_file.dna_hash().clone();

    let db = match msg {
        shared_types::Msg::DbSent(db) => Some(db),
        shared_types::Msg::NoDb => None,
        _ => unreachable!(),
    };

    if let Some(db) = db {
        dbg!(&db);
        let root_path = conductor.db_path().to_path_buf();
        let path = root_path.join("dht");
        std::fs::create_dir(&path).ok();
        let path = path.join(format!("dht-{}.sqlite3", dna_hash));
        let bytes = std::fs::copy(&db, &path).unwrap();
        println!(
            "Copied {} MB from {} to {} database.",
            (bytes / 1024) / 1024,
            db.display(),
            path.display()
        );
    }

    let msg = rx.recv().await.unwrap();
    let infos = match msg {
        shared_types::Msg::Infos(infos) => infos,
        _ => unreachable!(),
    };
    p2p_put_all(&p2p_store, infos.iter()).await.unwrap();

    let app = conductor
        .setup_app_for_agent("app", alice, &[dna_file.clone()])
        .await
        .unwrap();

    let cell = app.into_cells();

    let env = cell[0].dht_db();

    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    if num_other_agents > 0 {
        let agents = SweetAgents::get(keystore, num_other_agents).await;
        let apps = conductor
            .setup_app_for_agents("app2", agents.iter(), &[dna_file.clone()])
            .await
            .unwrap();
        let cells: Vec<_> = apps.cells_flattened().into_iter().cloned().collect();
        let handle = conductor.inner_handle();

        tokio::task::spawn(async move {
            futures::stream::iter((0..num_other_agents).cycle())
                .for_each_concurrent(3, {
                    // let cells = cells.clone();
                    |state| {
                        let cell = cells[state].clone();
                        let handle = handle.clone();
                        async move {
                            let handle = SweetConductorHandle::from_handle(handle);
                            let r: chat::ChannelList = handle
                                .call(
                                    &cell.zome("chat"),
                                    "list_channels",
                                    chat::ChannelListInput {
                                        category: "any".to_string(),
                                    },
                                )
                                .await;
                            for channel in r.channels.into_iter() {
                                let _: chat::ListMessages = handle
                                    .call(
                                        &cell.zome("chat"),
                                        "list_messages",
                                        chat::ListMessagesInput {
                                            channel: channel.entry.clone(),
                                            earliest_seen: None,
                                            target_message_count: 20,
                                        },
                                    )
                                    .await;
                                tokio::time::sleep(std::time::Duration::from_secs(20)).await;
                            }
                            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                        }
                    }
                })
                .await;
        });
    }

    let ai = conductor
        .get_agent_infos(Some(cell[0].cell_id().clone()))
        .await
        .unwrap();
    eprintln!(
        "Alice {:.2} = {:.2}",
        alice_arc.coverage(),
        ai[0].storage_arc.coverage()
    );

    let mut hits_hist = Histogram::<u64>::new(3).unwrap();
    let mut miss_hist = Histogram::<u64>::new(3).unwrap();

    // let msg = rx.recv().await.unwrap();
    // let hashes = match msg {
    //     shared_types::Msg::Hashes(hashes) => hashes,
    //     _ => unreachable!(),
    // };
    let hashes: Vec<HeaderHash> = vec![];

    match dna_type {
        DnaType::Blank => {
            loop {
                for hash in &hashes {
                    eprintln!("{}", holochain::test_utils::query_integration(env).await);
                    // let infos = conductor.get_agent_infos(None).await.unwrap();
                    // eprintln!("{:?}", infos);
                    let s = std::time::Instant::now();
                    let r: Option<Element> = if inline_zome {
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
        DnaType::EChat => {
            let env = cell[0].dht_db();
            let mut chans_hist = Histogram::<u64>::new(3).unwrap();
            let mut found_chans_hist = Histogram::<u64>::new(3).unwrap();
            let mut msgs_hist = Histogram::<u64>::new(3).unwrap();
            let mut found_msgs_hist = Histogram::<u64>::new(3).unwrap();
            let mut new_chans_hist = Histogram::<u64>::new(3).unwrap();
            let mut new_msgs_hist = Histogram::<u64>::new(3).unwrap();
            let mut signals_hist = Histogram::<u64>::new(3).unwrap();
            loop {
                let s = std::time::Instant::now();

                let r: chat::ChannelList = conductor
                    .call(
                        &cell[0].zome("chat"),
                        "list_channels",
                        chat::ChannelListInput {
                            category: "any".to_string(),
                        },
                    )
                    .await;
                let elapsed = s.elapsed();
                let len = if r.channels.is_empty() {
                    1
                } else {
                    r.channels.len()
                };
                chans_hist
                    .record_n(elapsed.as_micros() as u64, len as u64)
                    .unwrap();
                found_chans_hist.record(len as u64).unwrap();

                print_counts(&found_chans_hist, "Found Channels:");
                print_hst(&chans_hist, "Channels:");

                let num_chans = r.channels.len();
                for (i, channel) in r.channels.into_iter().enumerate() {
                    let s = std::time::Instant::now();
                    let r: chat::ListMessages = conductor
                        .call(
                            &cell[0].zome("chat"),
                            "list_messages",
                            chat::ListMessagesInput {
                                channel: channel.entry.clone(),
                                earliest_seen: None,
                                target_message_count: 70,
                            },
                        )
                        .await;
                    let elapsed = s.elapsed();
                    let len = if r.messages.is_empty() {
                        1
                    } else {
                        r.messages.len()
                    };
                    msgs_hist
                        .record_n(elapsed.as_micros() as u64, len as u64)
                        .unwrap();

                    found_msgs_hist.record(len as u64).unwrap();

                    eprintln!(
                        "{:?}: {}. Channel {} out of {}",
                        elapsed,
                        r.messages.len(),
                        i,
                        num_chans
                    );
                    eprintln!("{}", holochain::test_utils::query_integration(env).await);

                    let content: [char; 300] = mock_network::types::make(|u| u.arbitrary());
                    let s = std::time::Instant::now();
                    let message_data: chat::MessageData = conductor
                        .call(
                            &cell[0].zome("chat"),
                            "create_message",
                            chat::MessageInput {
                                channel: channel.entry.clone(),
                                last_seen: r
                                    .messages
                                    .last()
                                    .map_or(chat::entries::message::LastSeen::First, |m| {
                                        chat::message::LastSeen::Message(m.entry_hash.clone())
                                    }),
                                entry: chat::Message {
                                    uuid: uuid::Uuid::new_v4().to_string(),
                                    content: content.into_iter().collect(),
                                },
                            },
                        )
                        .await;
                    let elapsed = s.elapsed();
                    new_msgs_hist.record(elapsed.as_micros() as u64).unwrap();
                    if signals {
                        let s = std::time::Instant::now();
                        let _: chat::SigResults = conductor
                            .call(
                                &cell[0].zome("chat"),
                                "signal_chatters",
                                chat::SignalMessageData {
                                    message_data,
                                    channel_data: channel,
                                },
                            )
                            .await;
                        let elapsed = s.elapsed();
                        signals_hist.record(elapsed.as_micros() as u64).unwrap();
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                print_counts(&found_msgs_hist, "Found Messages:");
                print_hst(&msgs_hist, "Messages:");
                print_hst(&new_msgs_hist, "New Messages:");

                let s = std::time::Instant::now();
                let _: chat::ChannelData = conductor
                    .call(
                        &cell[0].zome("chat"),
                        "create_channel",
                        chat::ChannelInput {
                            name: "test".into(),
                            entry: chat::entries::channel::Channel {
                                category: "other".into(),
                                uuid: uuid::Uuid::new_v4().to_string(),
                            },
                        },
                    )
                    .await;
                let elapsed = s.elapsed();

                new_chans_hist.record(elapsed.as_micros() as u64).unwrap();

                print_hst(&new_chans_hist, "New Channels:");
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }
}

fn print_hst(hist: &Histogram<u64>, name: &str) {
    let hist = hist.iter_quantiles(1).fold(
        (String::new(), Duration::default()),
        |(mut out, mut last), i| {
            let e = Duration::from_micros(i.value_iterated_to());
            out = format!("{}{:?}->{:?}: {} \n", out, last, e, i.count_at_value());
            last = e;
            (out, last)
        },
    );
    if !hist.0.is_empty() {
        eprintln!("{} {}", name, hist.0);
    }
}

fn print_counts(hist: &Histogram<u64>, name: &str) {
    let hist = hist
        .iter_quantiles(1)
        .fold((String::new(), 0), |(mut out, mut last), i| {
            let e = i.value_iterated_to();
            out = format!("{}{}->{}: {} \n", out, last, e, i.count_at_value());
            last = e;
            (out, last)
        });
    if !hist.0.is_empty() {
        eprintln!("{} {}", name, hist.0);
    }
}
