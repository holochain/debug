use kitsune_p2p::dependencies::url2::Url2;
use sessions::prelude::*;
use sessions::shared_types;
use std::path::PathBuf;
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
    } = Input::parse();
    dbg!(&dna_path);
    dbg!(&inline_zome);
    dbg!(&bootstrap);
    dbg!(&ideal_length);
    dbg!(&dna_type);
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

    let envs = test_environments_in("/tmp/");
    let keystore = envs.keystore().clone();
    let handle = SweetConductor::handle_from_existing(&envs, &config, &[]).await;
    let mut conductor = SweetConductor::new(handle, envs, config.clone()).await;

    let alice = SweetAgents::one(keystore).await;
    let alice_arc = DhtArc::with_coverage(alice.get_loc(), ideal_length);

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
        let root_path = conductor.envs().path().to_path_buf();
        let path = root_path.join("dht");
        std::fs::create_dir(&path).unwrap();
        let path = path.join(format!("dht-{}.sqlite3", dna_hash));
        let bytes = std::fs::copy(&db, &path).unwrap();
        println!(
            "Copied {} MB from {} to {} database.",
            bytes,
            db.display(),
            path.display()
        );
    }

    let app = conductor
        .setup_app_for_agent("app", alice, &[dna_file.clone()])
        .await
        .unwrap();

    let cell = app.into_cells();

    let env = cell[0].dht_env();

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

    let mut hits_hist = Histogram::<u64>::new(3).unwrap();
    let mut miss_hist = Histogram::<u64>::new(3).unwrap();

    dbg!();
    let msg = rx.recv().await.unwrap();
    dbg!();
    let hashes = match msg {
        shared_types::Msg::Hashes(hashes) => hashes,
        _ => unreachable!(),
    };
    dbg!();
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
            let env = cell[0].dht_env();
            let mut chans_hist = Histogram::<u64>::new(3).unwrap();
            let mut msgs_hist = Histogram::<u64>::new(3).unwrap();
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
                chans_hist
                    .record_n(elapsed.as_micros() as u64, r.channels.len() as u64)
                    .unwrap();
                let hist = chans_hist.iter_quantiles(1).fold(
                    (String::new(), Duration::default()),
                    |(mut out, mut last), i| {
                        let e = Duration::from_micros(i.value_iterated_to());
                        out = format!("{}{:?}->{:?}: {} \n", out, last, e, i.count_at_value());
                        last = e;
                        (out, last)
                    },
                );
                if !hist.0.is_empty() {
                    eprintln!("Channels {}", hist.0);
                }
                let num_chans = r.channels.len();
                for (i, channel) in r.channels.into_iter().enumerate() {
                    let s = std::time::Instant::now();
                    let r: chat::ListMessages = conductor
                        .call(
                            &cell[0].zome("chat"),
                            "list_messages",
                            chat::ListMessagesInput {
                                channel: channel.entry,
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
                    eprintln!(
                        "{:?}: {}. Channel {} out of {}",
                        elapsed,
                        r.messages.len(),
                        i,
                        num_chans
                    );
                    eprintln!("{}", holochain::test_utils::query_integration(env).await);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                let hist = msgs_hist.iter_quantiles(1).fold(
                    (String::new(), Duration::default()),
                    |(mut out, mut last), i| {
                        let e = Duration::from_micros(i.value_iterated_to());
                        out = format!("{}{:?}->{:?}: {}\n", out, last, e, i.count_at_value());
                        last = e;
                        (out, last)
                    },
                );
                if !hist.0.is_empty() {
                    eprintln!("Messages {}", hist.0);
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }
}
