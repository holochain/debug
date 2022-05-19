use clap::ArgEnum;
use clap::Parser;
use controller::remote;
use futures::StreamExt;
use holochain_state::prelude::DatabaseResult;
use holochain_types::prelude::DbKindDht;
use holochain_types::prelude::DnaFile;
use kitsune_p2p::agent_store::AgentInfoSigned;
use kitsune_p2p::dependencies::url2;
use kitsune_p2p::dht_arc::DhtArc;
use mock_network::agent_info::GenerateAgentInfo;
use mock_network::agent_info::GenerateQuicAgents;
use mock_network::hdk::GenerateHdk;
use mock_network::types::*;
use mock_network::*;
use observability::tracing::*;
use sessions::prelude::*;
use sessions::run_bootstrap;
use sessions::shared_types;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

struct MyData(Vec<u8>);

impl<'a> arbitrary::Arbitrary<'a> for MyData {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self(u.bytes(400)?.to_vec()))
    }
}

#[derive(Debug, ArgEnum, Clone, Copy)]
enum DataStream {
    MyData,
    EChat,
}

#[derive(Debug, Parser)]
struct Input {
    #[clap(long)]
    inline_zome: bool,
    #[clap(long, default_value_t = 10)]
    num_agents: usize,
    #[clap(long, default_value_t = 70)]
    num_chain_items: usize,
    #[clap(long, default_value_t = 10)]
    num_locals: usize,
    #[clap(long)]
    override_dna: Option<PathBuf>,
    #[clap(long, arg_enum)]
    data_stream: DataStream,
    #[clap(long, default_value_t = 0)]
    num_other_agents: usize,
    #[clap(long)]
    signals: bool,
    #[clap(long)]
    reflect_signals: bool,
}

struct Remote {
    session: remote::Session,
    scp: remote::Scp,
    remote_dna_path: PathBuf,
}

#[tokio::main]
async fn main() {
    let Input {
        inline_zome,
        num_agents,
        num_chain_items,
        num_locals,
        override_dna,
        data_stream,
        num_other_agents,
        signals,
        reflect_signals,
    } = Input::parse();
    let _ = observability::test_run();

    let addr = run_bootstrap().await;
    let boot_url = url2::url2!("http://{}:{}", addr.ip(), addr.port());

    let (dna_file, dna_path) = get_dna(inline_zome, &override_dna).await;

    let dna_hash = dna_file.dna_hash().clone();
    dbg!(&dna_hash);

    let remotes = install_remotes(inline_zome, &override_dna).await;

    let (gen_data, agent_info, quic) =
        generate_data(num_agents, num_chain_items, &dna_hash, data_stream).await;

    let mut channel = mock_network::real_networked(agent_info.clone(), quic);
    let gossip_data_requests = mock_network::DataRequestsBuilder::default().data(gen_data.clone());

    let local_ws_urls = run_locals(
        num_locals,
        num_agents,
        boot_url.clone(),
        inline_zome,
        &dna_path,
        data_stream,
        num_other_agents,
        signals,
    )
    .await;
    let remote_ws_urls = run_remotes(
        &remotes,
        num_agents,
        boot_url.clone(),
        inline_zome,
        data_stream,
        num_other_agents,
        signals,
    )
    .await;

    let mut remote_handles = Vec::new();
    let mut remote_nodes = Vec::new();
    for (handle, rx) in remote_ws_urls {
        remote_handles.push(handle);
        let url = rx.await.unwrap();
        remote_nodes.push(url);
    }

    let mut local_nodes = Vec::new();

    for rx in local_ws_urls {
        let url = rx.await.unwrap();
        local_nodes.push(url);
    }

    let gossip_data = mock_network::Gossip::new(
        Default::default(),
        &dna_hash,
        gen_data.data.keys().cloned(),
        gossip_data_requests,
    )
    .await;

    let hashes: Vec<_> = gen_data
        .data
        .values()
        .flat_map(|v| {
            v.iter()
                .map(|el| el.header_address())
                .cloned()
                .collect::<Vec<_>>()
        })
        .collect();
    let hashes = Arc::new(hashes);

    send_db_and_hashes(
        &remotes,
        remote_nodes,
        local_nodes,
        &dna_hash,
        gen_data.clone(),
        hashes,
    )
    .await;

    tokio::spawn({
        let agent_info = agent_info.clone();

        async move {
            for info in agent_info.iter() {
                controller::bootstrap::put_info(boot_url.clone(), info.clone()).await;
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    });

    let mut new_peers = HashMap::new();
    let mut send_signal_time = std::time::Instant::now();
    while let Some((msg, respond)) = channel.next().await {
        let to_agent = msg.to_agent.as_key().unwrap();
        let from_agent = msg.from_agent.clone();
        // eprintln!("{} -> {}", from_agent, to_agent);
        let s = std::time::Instant::now();
        let msg_str = format!("{:?}", msg.msg);
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
            HolochainP2pMockMsg::Wire {
                msg: WireMessage::GetLinks { link_key, .. },
                ..
            } => {
                let r = gen_data.get_links(to_agent.as_ref(), link_key);
                let r: Vec<u8> = UnsafeBytes::from(SerializedBytes::try_from(r).unwrap()).into();
                let msg = HolochainP2pMockMsg::CallResp(r.into());
                respond.unwrap().respond(msg);
            }
            HolochainP2pMockMsg::Wire {
                msg:
                    WireMessage::CallRemote {
                        zome_name,
                        fn_name,
                        from_agent: fa,
                        data,
                        ..
                    },
                to_agent: ta,
                dna,
            } => {
                // println!("zome: {:?}, fn {:?}", zome_name, fn_name);
                use holochain_p2p::AgentPubKeyExt;
                if reflect_signals
                    && new_peers.contains_key(&fa.to_kitsune())
                    && send_signal_time.elapsed().as_secs() >= 1
                {
                    let msg = HolochainP2pMockMsg::Wire {
                        dna: dna.clone(),
                        to_agent: fa,
                        msg: WireMessage::CallRemote {
                            zome_name,
                            fn_name,
                            from_agent: ta,
                            cap_secret: None,
                            data,
                        },
                    };
                    channel
                        .send(msg.addressed(to_agent.clone(), from_agent.clone()))
                        .await;
                    send_signal_time = std::time::Instant::now();
                }
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
                let from = channel.real_node_address(&from_agent).cert.clone();
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
                let info = gen_data.peer_get(&to_agent, request.clone(), &new_peers);
                match info {
                    Some(info) => {
                        respond.unwrap().respond(HolochainP2pMockMsg::PeerGetResp(
                            kwire::PeerGetResp {
                                agent_info_signed: info,
                            },
                        ));
                    }
                    None => {
                        respond
                            .unwrap()
                            .respond(HolochainP2pMockMsg::Failure(format!(
                                "Missing peer {}",
                                request
                            )));
                    }
                }
            }
            HolochainP2pMockMsg::PeerQuery(kwire::PeerQuery { basis_loc, .. }) => {
                let info = gen_data.peer_query(&to_agent, basis_loc);
                respond.unwrap().respond(HolochainP2pMockMsg::PeerQueryResp(
                    kwire::PeerQueryResp { peer_list: info },
                ));
            }
            HolochainP2pMockMsg::PublishedAgentInfo { info, .. } => {
                new_peers.insert(info.agent.clone(), info);
            }
            _ => channel.default_response(msg, respond).await,
        }
        let elapsed = s.elapsed();
        if elapsed.as_millis() > 200 {
            observability::tracing::error!("Slow message took {:?}: {:?}", elapsed, msg_str);
        }
    }
}

async fn dump_data(
    node: usize,
    dna_hash: &DnaHash,
    gen_data: Arc<GeneratedData>,
    alice_arc: DhtArc,
) -> PathBuf {
    let s = std::time::Instant::now();

    let env = test_in_mem_db(DbKindDht(Arc::new(dna_hash.clone())));

    let ops_alice_holds: HashSet<_> = gen_data
        .agent_loc_time_idx
        .iter()
        .filter(|((_, l, _), _)| alice_arc.contains(*l))
        .flat_map(|(_, h)| h.clone())
        .collect();

    env.async_commit(move |txn| {
        let ops = gen_data
            .agent_khash_idx
            .values()
            .flatten()
            .filter(|(khash, _)| ops_alice_holds.contains(khash.as_ref()))
            .map(|(_, (op_type, element))| (element.as_ref(), *op_type));
        insert_ops_as_authority(txn, ops);
        DatabaseResult::Ok(())
    })
    .await
    .unwrap();

    warn!(data_inserted_in = ?s.elapsed());
    let s = std::time::Instant::now();

    let db = dump_database(&env, &node.to_string());
    warn!(data_dumped_in = ?s.elapsed());
    db
}

async fn get_dna(inline_zome: bool, override_dna: &Option<PathBuf>) -> (DnaFile, PathBuf) {
    if inline_zome {
        let entry_def = EntryDef::default_with_id("entrydef");
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
        (dna_file, Default::default())
    } else {
        let dna_path = match override_dna {
            Some(dna) => dna.clone(),
            None => {
                sessions::build::build_dna(
                    &Path::new("/home/freesig/holochain/debug/test_app/test_zome/Cargo.toml"),
                    "test_app",
                )
                .await
            }
        };

        let dna_file = SweetDnaFile::from_bundle(&dna_path).await.unwrap();
        (dna_file, dna_path)
    }
}

async fn install_remotes(inline_zome: bool, override_dna: &Option<PathBuf>) -> Vec<Remote> {
    dbg!();
    let session = remote::Session::new("freesig", "192.168.1.3", "~/tests/").await;
    dbg!();
    session.kill("test-bin-10-3-22").await;
    dbg!();
    let scp = remote::Scp::new("freesig", "192.168.1.3");
    dbg!();
    scp.install_test("~/tests/", "test-bin-10-3-22").await;
    dbg!();

    let remote_dna_path = if !inline_zome {
        match override_dna {
            Some(dna) => {
                scp.install(&dna, "~/tests/").await;
                PathBuf::from("~/tests/").join(dna.file_name().unwrap())
            }
            None => {
                scp.install_dna(
                    "~/tests/",
                    "/home/freesig/holochain/debug/test_app/test_zome/Cargo.toml",
                    "test_app",
                )
                .await
            }
        }
    } else {
        PathBuf::default()
    };

    vec![Remote {
        session,
        scp,
        remote_dna_path,
    }]
}

async fn run_locals(
    num_locals: usize,
    num_agents: usize,
    boot_url: url2::Url2,
    inline_zome: bool,
    dna_path: &Path,
    data_stream: DataStream,
    num_other_agents: usize,
    signals: bool,
) -> Vec<tokio::sync::oneshot::Receiver<url2::Url2>> {
    controller::local::kill("test-bin-10-3-22").await;
    let mut ws_urls = Vec::new();
    let ideal_length =
        mock_network::agent_info::ideal_target(num_agents + num_locals, DhtArc::full(0u32.into()))
            .coverage();
    let ils = ideal_length.to_string();
    let bus = boot_url.to_string();
    let rds = dna_path.display().to_string();
    let nos = num_other_agents.to_string();
    let mut args = vec![
        "--ideal-length",
        ils.as_str(),
        "--bootstrap",
        bus.as_str(),
        "--dna-path",
        rds.as_str(),
        "--num-other-agents",
        nos.as_str(),
    ];
    if inline_zome {
        args.push("--inline-zome");
    }
    if signals {
        args.push("--signals");
    }
    match data_stream {
        DataStream::MyData => {
            args.push("--dna-type");
            args.push("blank");
        }
        DataStream::EChat => {
            args.push("--dna-type");
            args.push("e-chat");
        }
    }
    for i in 0..num_locals {
        let mut out =
            controller::local::run("test-bin-10-3-22", &args, vec![("RUST_LOG", "warn")]).await;
        let stream = out.output_stream();
        let (tx, rx) = tokio::sync::oneshot::channel();
        ws_urls.push(rx);
        let mut tx = Some(tx);
        tokio::spawn(async move {
            let _out = out;
            stream
                .for_each(|line| {
                    if tx.is_some() && line.starts_with("[STDOUT] [[URL]]") {
                        if let Some(url) = line
                            .split(' ')
                            .skip(2)
                            .next()
                            .and_then(|line| url2::Url2::try_parse(line).ok())
                        {
                            tx.take().unwrap().send(url).unwrap();
                        }
                    }
                    futures::future::ready(eprintln!("LOCAL {}: {}", i, line))
                })
                .await
        });
    }
    ws_urls
}

async fn run_remotes<'session>(
    remotes: &'session Vec<Remote>,
    num_agents: usize,
    boot_url: url2::Url2,
    inline_zome: bool,
    data_stream: DataStream,
    num_other_agents: usize,
    signals: bool,
) -> Vec<(
    controller::remote::Output<'session>,
    tokio::sync::oneshot::Receiver<url2::Url2>,
)> {
    let mut ws_urls = Vec::new();
    for (
        i,
        Remote {
            session,
            remote_dna_path,
            ..
        },
    ) in remotes.iter().enumerate()
    {
        let ideal_length =
            mock_network::agent_info::ideal_target(num_agents + 1, DhtArc::full(0u32.into()))
                .coverage();
        let ils = ideal_length.to_string();
        let bus = boot_url.to_string();
        let rds = remote_dna_path.display().to_string();
        let nos = num_other_agents.to_string();
        let mut args = vec![
            "--ideal-length",
            ils.as_str(),
            "--bootstrap",
            bus.as_str(),
            "--dna-path",
            rds.as_str(),
            "--num-other-agents",
            nos.as_str(),
        ];
        if inline_zome {
            args.push("--inline-zome");
        }
        if signals {
            args.push("--signals");
        }
        match data_stream {
            DataStream::MyData => {
                args.push("--dna-type");
                args.push("blank");
            }
            DataStream::EChat => {
                args.push("--dna-type");
                args.push("e-chat");
            }
        }
        dbg!();
        let mut out = session
            .run(
                "test-bin-10-3-22",
                &args,
                // &["RUST_LOG=warn,kitsune_p2p_types::tx2::tx2_pool_promote[]=debug"],
                // &["RUST_LOG=warn,kitsune_p2p_types::tx2::tx2_api[]=trace,holochain_p2p::spawn::actor[]=trace"],
                // &[
                //     "RUST_LOG=warn,[gossip_metrics]=trace,[process_incoming_recent]=trace,[process_outgoing_recent]=trace",
                //     "GOSSIP_METRICS=trace",
                // ],
                // &["RUST_LOG=warn,[get]=debug,[get_links]=debug"],
                &["RUST_LOG=warn", "RUST_BACKTRACE=1"],
            )
            .await;
        dbg!();
        let stream = out.output_stream();
        let (tx, rx) = tokio::sync::oneshot::channel();
        ws_urls.push((out, rx));
        let mut tx = Some(tx);
        tokio::spawn(async move {
            stream
                .for_each(|line| {
                    if tx.is_some() && line.starts_with("[STDOUT] [[URL]]") {
                        if let Some(url) = line
                            .split(' ')
                            .skip(2)
                            .next()
                            .and_then(|line| url2::Url2::try_parse(line).ok())
                        {
                            tx.take().unwrap().send(url).unwrap();
                        }
                    }
                    futures::future::ready(eprintln!("REMOTE {}: {}", i, line))
                })
                .await;
            dbg!();
        });
    }
    ws_urls
}

async fn generate_data(
    num_agents: usize,
    num_chain_items: usize,
    dna_hash: &DnaHash,
    data_stream: DataStream,
) -> (
    Arc<GeneratedData>,
    Arc<Vec<AgentInfoSigned>>,
    mock_network::network::quic::Quic,
) {
    use rayon::prelude::*;
    let keystore = holochain_keystore::test_keystore::spawn_test_keystore()
        .await
        .unwrap();
    let s = std::time::Instant::now();
    let data = match data_stream {
        DataStream::MyData => {
            let agent_data = tokio::task::spawn_blocking({
                let dna_hash = dna_hash.clone();
                let keystore = keystore.clone();
                move || {
                    dbg!(s.elapsed());
                    let agent_data = (0..num_agents)
                        .into_par_iter()
                        .map(move |_| {
                            let data = std::iter::repeat_with(move || {
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
                                        EntryVisibility::Public,
                                    ))),
                                    entry_hash: Some(entry_hash),
                                    ..Default::default()
                                };
                                ChainData {
                                    header: header.into(),
                                    entry,
                                }
                            })
                            .take(num_chain_items);

                            Generate {
                                keystore: keystore.clone(),
                                data,
                                dna_hash: dna_hash.clone(),
                                genesis_settings: GenesisBuilder::default(),
                            }
                        })
                        .collect();
                    agent_data
                }
            })
            .await
            .unwrap();
            let data = GenerateBatch { agent_data };

            dbg!(s.elapsed());
            data.make().await
        }
        DataStream::EChat => {
            let (hdks, sign_jh) = GenerateHdk {
                keystore: keystore.clone(),
                dna_hash: dna_hash.clone(),
                genesis_settings: (0..num_agents)
                    .map(|_| {
                        let mut settings = GenesisBuilder::default();
                        settings.start_time(Some(
                            SystemTime::now()
                                .checked_sub(std::time::Duration::from_secs(60 * 60 * 60 * 7))
                                .unwrap(),
                        ));
                        settings
                    })
                    .collect(),
                zome_id: 0.into(),
                entry_defs: chat::entry_defs(()).unwrap(),
                time_step: std::time::Duration::from_secs(60),
                sys_start_time: SystemTime::now()
                    .checked_sub(std::time::Duration::from_secs(60 * 60 * 60 * 7))
                    .unwrap(),
            }
            .make()
            .await;
            let num_channels = 100;
            let channels = Arc::new(
                (0..num_channels)
                    .map(|_| chat::entries::channel::Channel {
                        category: "any".into(),
                        uuid: uuid::Uuid::new_v4().to_string(),
                    })
                    .collect::<Vec<_>>(),
            );
            let r = tokio::task::spawn_blocking({
                move || {
                    hdks.into_par_iter()
                        .enumerate()
                        .map(|(i, hdk)| {
                            let channel = channels[i % num_channels].clone();
                            chat::set_hdk(hdk.clone());
                            if i < num_channels {
                                let channel_input = chat::entries::channel::ChannelInput {
                                    name: "test".into(),
                                    entry: channel.clone(),
                                };
                                chat::entries::channel::handlers::create_channel(channel_input)
                                    .unwrap();
                            }
                            let mut md: Option<chat::MessageData> = None;
                            for _ in 0..num_chain_items {
                                let last_seen = match md {
                                    Some(md) => {
                                        chat::entries::message::LastSeen::Message(md.entry_hash)
                                    }
                                    None => chat::entries::message::LastSeen::First,
                                };
                                let content: [char; 300] = types::make(|u| u.arbitrary());
                                let msg = chat::entries::message::MessageInput {
                                    last_seen,
                                    channel: channel.clone(),
                                    entry: chat::entries::message::Message {
                                        uuid: uuid::Uuid::new_v4().to_string(),
                                        content: content.into_iter().collect(),
                                    },
                                };
                                md = Some(
                                    chat::entries::message::handlers::create_message(msg).unwrap(),
                                );
                            }
                            chat::entries::message::handlers::refresh_chatter().unwrap();
                            chat::set_hdk(chat::ErrHdk);
                            hdk.make()
                        })
                        .collect()
                }
            })
            .await
            .unwrap();
            sign_jh.await.unwrap();
            r
        }
    };

    dbg!(s.elapsed());
    let mut settings = mock_network::agent_info::SettingsBuilder::default();
    let ideal_length =
        mock_network::agent_info::ideal_target(num_agents + 1, DhtArc::full(0u32.into()))
            .coverage();
    settings.dht_storage_arc_coverage(ideal_length);
    let agents = GenerateAgentInfo {
        keystore: &keystore,
        agent_keys: data.keys(),
        dna_hash: dna_hash.clone(),
        settings,
    };
    let (agent_info, quic) = GenerateQuicAgents { agents }.make().await;

    dbg!(s.elapsed());
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
    dbg!(s.elapsed());
    let agent_info = Arc::new(agent_info);
    let gen_data = mock_network::GeneratedData::new(data.clone(), agent_info.clone()).await;
    dbg!(s.elapsed());
    (gen_data, agent_info, quic)
}

async fn send_db_and_hashes(
    remotes: &Vec<Remote>,
    remote_nodes: Vec<url2::Url2>,
    local_nodes: Vec<url2::Url2>,
    dna_hash: &DnaHash,
    gen_data: Arc<GeneratedData>,
    hashes: Arc<Vec<HeaderHash>>,
) {
    let nodes: Vec<_> = futures::stream::iter(
        remote_nodes
            .into_iter()
            .zip(remotes.iter())
            .map(|(url, r)| (Some(r.scp.clone()), url))
            .chain(local_nodes.into_iter().map(|u| (None, u)))
            .enumerate(),
    )
    .then(|(i, (remote, url))| {
        let gen_data = gen_data.clone();
        let _hashes = hashes.clone();
        async move {
            let (tx, mut rx) = controller::connect::connect(url).await;

            let (msg, resp) = rx.recv().await.unwrap();
            let alice_arc = match msg {
                shared_types::Msg::DhtArc(a) => a,
                _ => unreachable!(),
            };
            // if remote.is_some() {
            let db_path = dump_data(i, &dna_hash, gen_data.clone(), alice_arc).await;
            let remote_db_path = match remote {
                Some(scp) => scp.install_db("/home/freesig/tests/", dbg!(db_path)).await,
                None => db_path,
            };
            let msg = shared_types::Msg::DbSent(remote_db_path);
            // let msg = shared_types::Msg::NoDb;
            resp.respond(msg.try_into().unwrap()).await.unwrap();
            // } else {
            //     let msg = shared_types::Msg::NoDb;
            //     resp.respond(msg.try_into().unwrap()).await.unwrap();
            //
            // }
            (tx, rx, gen_data, alice_arc)

            // let msg = shared_types::Msg::Hashes((*hashes).clone());
            // tx.signal(msg).await.unwrap();
        }
    })
    .collect()
    .await;

    for (mut tx, _rx, gen_data, arc) in nodes {
        let infos: Vec<_> = gen_data
            .agent_info
            .iter()
            .filter(|info| arc.contains(info.storage_arc.start_loc()))
            .cloned()
            .collect();
        let msg = shared_types::Msg::Infos(infos);
        tx.signal(msg).await.unwrap();
    }
}
