use derive_builder::Builder;
use futures::FutureExt;
use holochain_p2p::dht_arc::DhtLocation;
use holochain_p2p::AgentPubKeyExt;
use holochain_p2p::DnaHashExt;
use holochain_p2p::WireDhtOpData;
use holochain_types::prelude::*;
use kitsune_p2p::agent_store::AgentInfoSigned;
use kitsune_p2p::event::FetchOpDataEvt;
use kitsune_p2p::event::KitsuneP2pEventHandlerResult;
use kitsune_p2p::event::QueryAgentsEvt;
use kitsune_p2p::event::QueryOpHashesEvt;
use kitsune_p2p::event::TimeWindowInclusive;
use kitsune_p2p::gossip::sharded_gossip::GossipType;
use kitsune_p2p::gossip::sharded_gossip::ShardedGossipLocal;
use kitsune_p2p::gossip::sharded_gossip::ShardedGossipLocalState;
use kitsune_p2p::GossipModuleType;
use kitsune_p2p::KOp;
use kitsune_p2p::KitsuneAgent;
use kitsune_p2p::KitsuneOpData;
use kitsune_p2p::KitsuneOpHash;
use kitsune_p2p::KitsuneSpace;
use kitsune_p2p_types::config::KitsuneP2pTuningParams;
use kitsune_p2p_types::Tx2Cert;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use super::*;

#[derive(Debug)]
struct GossipState {
    recent: ShardedGossipLocal,
    historical: ShardedGossipLocal,
}

type CurrentAgentShare = Arc<CurrentAgent>;

struct CurrentAgent {
    agent_index: HashMap<Arc<AgentPubKey>, usize>,
    agents: Vec<Arc<AgentPubKey>>,
    current: AtomicUsize,
}

pub struct Gossip {
    agents: HashMap<Arc<AgentPubKey>, GossipState>,
    current_agent: CurrentAgentShare,
}

#[derive(Builder)]
#[builder(pattern = "owned")]
pub struct DataRequests {
    #[builder(default = "default_query_agents(&self.data)")]
    query_agents: Box<
        dyn FnMut(
                QueryAgentsEvt,
                &AgentPubKey,
            ) -> KitsuneP2pEventHandlerResult<Vec<AgentInfoSigned>>
            + Send,
    >,
    #[builder(default = "default_query_op_hashes(&self.data)")]
    query_op_hashes: Box<
        dyn FnMut(
                QueryOpHashesEvt,
                &AgentPubKey,
            ) -> KitsuneP2pEventHandlerResult<
                Option<(Vec<Arc<KitsuneOpHash>>, TimeWindowInclusive)>,
            > + Send,
    >,
    #[builder(default = "default_fetch_op_data(&self.data)")]
    fetch_op_data: Box<
        dyn FnMut(
                FetchOpDataEvt,
                &AgentPubKey,
            ) -> KitsuneP2pEventHandlerResult<Vec<(Arc<KitsuneOpHash>, KOp)>>
            + Send,
    >,
    #[builder(default = "default_gossip(&self.data)")]
    gossip: Box<
        dyn FnMut(Arc<KitsuneSpace>, Vec<KOp>, &AgentPubKey) -> KitsuneP2pEventHandlerResult<()>
            + Send,
    >,
    #[allow(dead_code)]
    #[builder(default)]
    data: GeneratedShare,
}

impl Gossip {
    pub async fn new(
        tuning_params: KitsuneP2pTuningParams,
        dna: &DnaHash,
        agents: impl Iterator<Item = Arc<AgentPubKey>>,
        data_requests: DataRequestsBuilder,
    ) -> Self {
        let agents: Vec<_> = agents.collect();
        let current_agent = CurrentAgentShare::new(CurrentAgent::new(agents.clone()));
        let mock = data_requests.build().unwrap().build(current_agent.clone());
        let (evt_sender, _) = kitsune_p2p::test_util::spawn_handler(mock).await;
        Self {
            current_agent,
            agents: agents
                .into_iter()
                .map(|a| {
                    let ka = a.to_kitsune();
                    let space = dna.to_kitsune();
                    let mut local_agents = HashSet::new();
                    local_agents.insert(ka);
                    (
                        a,
                        GossipState::new(
                            tuning_params.clone(),
                            space,
                            local_agents,
                            evt_sender.clone(),
                        ),
                    )
                })
                .collect(),
        }
    }

    pub async fn process_msg(
        &self,
        from: Tx2Cert,
        to: &AgentPubKey,
        module: GossipModuleType,
        msg: ShardedGossipWire,
    ) -> Vec<ShardedGossipWire> {
        self.current_agent.set(to);
        let state = &self.agents[to];
        match module {
            GossipModuleType::Simple => todo!(),
            GossipModuleType::ShardedRecent => state
                .recent
                .process_incoming(from.clone(), msg)
                .await
                .unwrap(),
            GossipModuleType::ShardedHistorical => state
                .historical
                .process_incoming(from.clone(), msg)
                .await
                .unwrap(),
        }
    }
}

impl GossipState {
    fn new(
        tuning_params: KitsuneP2pTuningParams,
        space: Arc<KitsuneSpace>,
        local_agents: HashSet<Arc<KitsuneAgent>>,
        evt_sender: futures::channel::mpsc::Sender<kitsune_p2p::event::KitsuneP2pEvent>,
    ) -> Self {
        let state = ShardedGossipLocalState::with_local_agents(local_agents.clone());
        let recent = ShardedGossipLocal::new_test(
            GossipType::Recent,
            tuning_params.clone(),
            space.clone(),
            evt_sender.clone(),
            state,
        );
        let state = ShardedGossipLocalState::with_local_agents(local_agents);
        let historical = ShardedGossipLocal::new_test(
            GossipType::Historical,
            tuning_params,
            space,
            evt_sender,
            state,
        );
        Self { recent, historical }
    }
}

fn default_query_agents(
    data: &Option<GeneratedShare>,
) -> Box<
    dyn FnMut(QueryAgentsEvt, &AgentPubKey) -> KitsuneP2pEventHandlerResult<Vec<AgentInfoSigned>>
        + Send,
> {
    let data = data.clone();
    Box::new(move |query, agent| {
        let QueryAgentsEvt {
            agents,
            window,
            arc_set,
            near_basis,
            ..
        } = query;
        let out =
            match &data {
                Some(data) => {
                    let all_held_agents = &data.all_held_agents[agent];
                    let mut all_held_agents =
                        if arc_set.is_some() || window.is_some() || agents.is_some() {
                            all_held_agents
                                .iter()
                                .filter(|info| {
                                    arc_set.as_ref().map_or(true, |set| {
                                        set.contains(info.storage_arc.center_loc())
                                    }) && window.as_ref().map_or(true, |window| {
                                        window.contains(&Timestamp::from_micros(
                                            (info.signed_at_ms / 1000) as i64,
                                        ))
                                    }) && agents
                                        .as_ref()
                                        .map_or(true, |set| set.contains(&info.agent))
                                })
                                .cloned()
                                .collect()
                        } else {
                            all_held_agents.clone()
                        };
                    if let Some(nb) = near_basis {
                        all_held_agents.sort_unstable_by_key(|info| {
                            shortest_arc_distance(nb, info.storage_arc.center_loc())
                        });
                    }
                    all_held_agents
                }
                None => vec![],
            };
        Ok(async move { Ok(out) }.boxed().into())
    })
}
fn default_query_op_hashes(
    data: &Option<GeneratedShare>,
) -> Box<
    dyn FnMut(
            QueryOpHashesEvt,
            &AgentPubKey,
        ) -> KitsuneP2pEventHandlerResult<
            Option<(Vec<Arc<KitsuneOpHash>>, TimeWindowInclusive)>,
        > + Send,
> {
    let data = data.clone();
    Box::new(move |query, agent| {
        let QueryOpHashesEvt {
            arc_set, window, ..
        } = query;
        let agent = Arc::new(agent.clone());
        // Need hashes sorted by location -> timestamp
        let out = match &data {
            Some(data) => {
                let mut out = Vec::new();
                for interval in arc_set.intervals() {
                    if let Some((s, e)) = interval.to_bounds_grouped() {
                        if s <= e {
                            out.extend(
                                data.agent_loc_time_idx
                                    .range(
                                        (agent.clone(), s, window.start)
                                            ..(agent.clone(), e, window.end),
                                    )
                                    .flat_map(|(_, v)| v.clone()),
                            );
                        } else {
                            out.extend(
                                data.agent_loc_time_idx
                                    .range(
                                        (agent.clone(), DhtLocation::new(0), window.start)
                                            ..(agent.clone(), s, window.end),
                                    )
                                    .chain(data.agent_loc_time_idx.range(
                                        (agent.clone(), e, window.start)
                                            ..(
                                                agent.clone(),
                                                DhtLocation::new(u32::MAX),
                                                window.end,
                                            ),
                                    ))
                                    .flat_map(|(_, v)| v.clone()),
                            );
                        }
                    }
                }
                if !out.is_empty() {
                    Some((out, window.start..=window.end))
                } else {
                    None
                }
            }
            None => None,
        };
        Ok(async move { Ok(out) }.boxed().into())
    })
}
fn default_fetch_op_data(
    data: &Option<GeneratedShare>,
) -> Box<
    dyn FnMut(
            FetchOpDataEvt,
            &AgentPubKey,
        ) -> KitsuneP2pEventHandlerResult<Vec<(Arc<KitsuneOpHash>, KOp)>>
        + Send,
> {
    let data = data.clone();
    Box::new(move |query, agent| {
        let FetchOpDataEvt { op_hashes, .. } = query;
        // Need agent -> hash idx
        let mut out = Vec::with_capacity(op_hashes.len());
        if let Some(data) = &data {
            for hash in op_hashes {
                let (op_type, el) = &data.agent_khash_idx[agent][&hash];
                let op_data = DhtOp::from_type(
                    *op_type,
                    SignedHeader(el.header().clone(), el.signature().clone()),
                    if el.header().entry_type().map_or(true, |et| {
                        matches!(et.visibility(), EntryVisibility::Public)
                    }) {
                        el.entry().as_option().cloned()
                    } else {
                        None
                    },
                )
                .unwrap();
                out.push((
                    hash,
                    KitsuneOpData::new(WireDhtOpData { op_data }.encode().unwrap()),
                ));
            }
        }
        Ok(async move { Ok(out) }.boxed().into())
    })
}

fn default_gossip(
    _data: &Option<GeneratedShare>,
) -> Box<
    dyn FnMut(Arc<KitsuneSpace>, Vec<KOp>, &AgentPubKey) -> KitsuneP2pEventHandlerResult<()> + Send,
> {
    Box::new(move |_, _, _| Ok(async move { Ok(()) }.boxed().into()))
}

pub fn shortest_arc_distance<A: Into<DhtLocation>, B: Into<DhtLocation>>(a: A, b: B) -> u32 {
    // Turn into wrapped u32s
    let a = a.into().0;
    let b = b.into().0;
    std::cmp::min(a - b, b - a).0
}

impl DataRequests {
    fn build(self, current_agent: CurrentAgentShare) -> kitsune_p2p::MockKitsuneP2pEventHandler {
        let mut mock = kitsune_p2p::MockKitsuneP2pEventHandler::new();
        let DataRequests {
            mut query_agents,
            mut query_op_hashes,
            mut fetch_op_data,
            mut gossip,
            ..
        } = self;
        mock.expect_handle_query_agents().returning({
            let ca = current_agent.clone();
            move |arg| query_agents(arg, ca.get())
        });
        // .returning(move |_| Ok(async move { Ok(vec![]) }.boxed().into()));
        mock.expect_handle_query_op_hashes().returning({
            let ca = current_agent.clone();
            move |arg| query_op_hashes(arg, ca.get())
        });
        // .returning(move |_| Ok(async move { Ok(None) }.boxed().into()));
        mock.expect_handle_fetch_op_data().returning({
            let ca = current_agent.clone();
            move |arg| fetch_op_data(arg, ca.get())
        });
        // .returning(move |_| Ok(async move { Ok(vec![]) }.boxed().into()));
        mock.expect_handle_gossip().returning({
            let ca = current_agent.clone();
            move |arg1, arg2| gossip(arg1, arg2, ca.get())
        });
        // .returning(move |_, _| Ok(async move { Ok(()) }.boxed().into()));
        mock.expect_handle_put_agent_info_signed()
            .returning(move |_| Ok(async move { Ok(()) }.boxed().into()));
        mock
    }
}

impl CurrentAgent {
    fn new(agents: Vec<Arc<AgentPubKey>>) -> Self {
        let agent_index = agents
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, a)| (a, i))
            .collect();
        Self {
            agent_index,
            agents,
            current: Default::default(),
        }
    }

    fn set(&self, agent: &AgentPubKey) {
        self.current
            .store(self.agent_index[agent], std::sync::atomic::Ordering::SeqCst);
    }

    fn get(&self) -> &AgentPubKey {
        &self.agents[self.current.load(std::sync::atomic::Ordering::SeqCst)]
    }
}
