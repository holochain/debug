use std::collections::BTreeMap;

use holochain_p2p::dht_arc::DhtLocation;
use holochain_p2p::AgentPubKeyExt;
use holochain_p2p::DhtOpHashExt;
use holochain_types::prelude::*;
use kitsune_p2p::agent_store::AgentInfoSigned;
use kitsune_p2p::KitsuneOpHash;

use super::*;

pub type GeneratedShare = Arc<GeneratedData>;

#[derive(Default, Clone, Debug)]
pub struct GeneratedData {
    pub data: HashMap<Arc<AgentPubKey>, Vec<Arc<Element>>>,
    pub agent_info: Arc<Vec<AgentInfoSigned>>,
    pub agent_to_info: HashMap<Arc<AgentPubKey>, AgentInfoSigned>,
    pub all_held_agents: HashMap<Arc<AgentPubKey>, Vec<AgentInfoSigned>>,
    pub agent_loc_time_idx:
        BTreeMap<(Arc<AgentPubKey>, DhtLocation, Timestamp), Vec<Arc<KitsuneOpHash>>>,
    pub agent_khash_idx:
        HashMap<Arc<AgentPubKey>, HashMap<Arc<KitsuneOpHash>, (DhtOpType, Arc<Element>)>>,
    pub agent_any_idx: BTreeMap<(Arc<AgentPubKey>, Arc<AnyDhtHash>), Arc<Element>>,
}

impl GeneratedData {
    pub async fn new(
        data: HashMap<Arc<AgentPubKey>, Vec<Arc<Element>>>,
        agent_info: Arc<Vec<AgentInfoSigned>>,
    ) -> GeneratedShare {
        tokio::task::spawn_blocking(move || {
            use rayon::prelude::*;
            let s = std::time::Instant::now();
            let agent_to_info: HashMap<_, _> = agent_info
                .iter()
                .map(|info| {
                    (
                        data.get_key_value(&AgentPubKey::from_kitsune(&info.0.agent))
                            .unwrap()
                            .0
                            .clone(),
                        info.clone(),
                    )
                })
                .collect();
            dbg!(s.elapsed());
            let s = std::time::Instant::now();
            let all_held_agents: HashMap<_, Vec<_>> = agent_to_info
                .iter()
                .map(|(agent, info)| {
                    (
                        agent.clone(),
                        agent_info
                            .iter()
                            .filter(|other| {
                                info.storage_arc.contains(other.storage_arc.start_loc())
                            })
                            .cloned()
                            .collect(),
                    )
                })
                .collect();
            dbg!(s.elapsed());
            let s = std::time::Instant::now();
            let ops_by_loc: BTreeMap<
                DhtLocation,
                Vec<(DhtOpType, Arc<Element>, Arc<KitsuneOpHash>)>,
            > = data
                .values()
                .fold(BTreeMap::new(), |mut map: BTreeMap<_, Vec<_>>, el| {
                    let iter = el.iter().flat_map(|el| {
                        produce_op_lights_from_elements(vec![el.as_ref()])
                            .unwrap()
                            .into_iter()
                            .map(|op| {
                                (
                                    op.dht_basis().get_loc(),
                                    (
                                        op.get_type(),
                                        el.clone(),
                                        UniqueForm::op_hash(op.get_type(), el.header().clone())
                                            .unwrap()
                                            .1
                                            .into_kitsune(),
                                    ),
                                )
                            })
                    });
                    for (loc, v) in iter {
                        map.entry(loc).or_default().push(v);
                    }
                    map
                });
            fn check_public(op_type: &DhtOpType, el: &Element) -> bool {
                el.header().entry_type().map_or(true, |et| {
                    !(matches!(et.visibility(), EntryVisibility::Private)
                        && matches!(op_type, DhtOpType::StoreEntry))
                })
            }
            dbg!(s.elapsed());
            let s = std::time::Instant::now();
            let agent_loc_time_idx = agent_to_info
                .par_iter()
                .filter_map(|(agent, info)| {
                    info.storage_arc.to_bounds_grouped().map(|(s, e)| {
                        let mut map: BTreeMap<_, Vec<_>> = BTreeMap::new();
                        if s <= e {
                            let iter = ops_by_loc.range(s..=e).flat_map(|(l, v)| {
                                v.iter()
                                    .filter(|(op_type, el, _)| check_public(op_type, el.as_ref()))
                                    .map(|(_, el, hash)| {
                                        (*l, el.header().timestamp(), hash.clone())
                                    })
                            });
                            for (l, t, hash) in iter {
                                map.entry((agent.clone(), l, t)).or_default().push(hash);
                            }
                            map
                        } else {
                            let iter = ops_by_loc
                                .range(..=e)
                                .chain(ops_by_loc.range(s..))
                                .flat_map(|(l, v)| {
                                    v.iter()
                                        .filter(|(op_type, el, _)| {
                                            check_public(op_type, el.as_ref())
                                        })
                                        .map(|(_, el, hash)| {
                                            (*l, el.header().timestamp(), hash.clone())
                                        })
                                });
                            for (l, t, hash) in iter {
                                map.entry((agent.clone(), l, t)).or_default().push(hash);
                            }
                            map
                        }
                    })
                })
                .reduce(
                    || BTreeMap::new(),
                    |mut a, b| {
                        a.extend(b);
                        a
                    },
                );

            dbg!(s.elapsed());
            let s = std::time::Instant::now();
            let agent_khash_idx = agent_to_info.iter().fold(
                HashMap::new(),
                |mut map: HashMap<_, HashMap<_, _>>, (agent, info)| {
                    if let Some((s, e)) = info.storage_arc.to_bounds_grouped() {
                        if s <= e {
                            let iter = ops_by_loc.range(s..=e).flat_map(|(_, v)| {
                                v.iter()
                                    .filter(|(op_type, el, _)| check_public(op_type, el.as_ref()))
                                    .map(|(op_type, el, hash)| {
                                        (hash.clone(), (*op_type, el.clone()))
                                    })
                            });
                            map.entry(agent.clone()).or_default().extend(iter);
                        } else {
                            let iter = ops_by_loc
                                .range(..=e)
                                .chain(ops_by_loc.range(s..))
                                .flat_map(|(_, v)| {
                                    v.iter()
                                        .filter(|(op_type, el, _)| {
                                            check_public(op_type, el.as_ref())
                                        })
                                        .map(|(op_type, el, hash)| {
                                            (hash.clone(), (*op_type, el.clone()))
                                        })
                                });
                            map.entry(agent.clone()).or_default().extend(iter);
                        }
                    }
                    map
                },
            );

            dbg!(s.elapsed());
            let s = std::time::Instant::now();
            let agent_any_idx = agent_khash_idx
                .iter()
                .flat_map(|(agent, ops)| {
                    ops.values().filter_map(|(op_type, el)| match op_type {
                        DhtOpType::StoreEntry => Some((
                            (
                                agent.clone(),
                                Arc::new(AnyDhtHash::from(
                                    (*el.header().entry_hash().as_ref().unwrap()).clone(),
                                )),
                            ),
                            el.clone(),
                        )),
                        DhtOpType::StoreElement => Some((
                            (
                                agent.clone(),
                                Arc::new(AnyDhtHash::from(el.header_address().clone())),
                            ),
                            el.clone(),
                        )),
                        _ => None,
                    })
                })
                .collect();
            dbg!(s.elapsed());
            Arc::new(Self {
                data,
                agent_info,
                agent_to_info,
                all_held_agents,
                agent_loc_time_idx,
                agent_khash_idx,
                agent_any_idx,
            })
        })
        .await
        .unwrap()
    }
}
