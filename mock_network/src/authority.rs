use holochain_p2p::AgentPubKeyExt;
use std::sync::Arc;

use holochain_p2p::dht_arc::DhtLocation;
use holochain_types::dht_op::WireOps;
use holochain_types::prelude::*;
use kitsune_p2p::agent_store::AgentInfoSigned;
use kitsune_p2p::KitsuneAgent;

use super::*;

impl GeneratedData {
    pub fn get_links(&self, authority: &AgentPubKey, link_key: WireLinkKey) -> WireLinkOps {
        let WireLinkKey { base, tag, .. } = link_key;
        let creates = self
            .agent_khash_idx
            .get(authority)
            .unwrap()
            .values()
            .filter_map(|(op_type, element)| {
                match op_type {
                    DhtOpType::RegisterAddLink => match element.header() {
                        Header::CreateLink(CreateLink {
                            base_address,
                            tag: t,
                            ..
                        }) => {
                            if *base_address == base {
                                if let Some(tag) = &tag {
                                    if tag.0.len() <= t.0.len() {
                                        if t.0[..tag.0.len()] == tag.0 {
                                            let h = match element.header() {
                                                Header::CreateLink(cl) => cl,
                                                _ => unreachable!(),
                                            }
                                            .clone();
                                            return Some(WireCreateLink::condense(
                                                h,
                                                element.signature().clone(),
                                                ValidationStatus::Valid,
                                            ));
                                        }
                                    }
                                } else {
                                    let h = match element.header() {
                                        Header::CreateLink(cl) => cl,
                                        _ => unreachable!(),
                                    }
                                    .clone();
                                    return Some(WireCreateLink::condense(
                                        h,
                                        element.signature().clone(),
                                        ValidationStatus::Valid,
                                    ));
                                }
                            }
                        }
                        _ => (),
                    },
                    _ => (),
                }
                None
            })
            .collect();
        WireLinkOps {
            creates,
            deletes: Vec::with_capacity(0),
        }
    }

    pub fn get(&self, authority: AgentPubKey, hash: AnyDhtHash) -> WireOps {
        let hash_type = hash.hash_type().clone();
        // let contains = self
        //     .agent_to_info
        //     .get(&authority)
        //     .unwrap()
        //     .storage_arc
        //     .contains(hash.get_loc());
        // eprintln!("{} holds {:?} {:?}", &authority, contains, hash_type);

        let el = self
            .agent_any_idx
            .get(&(Arc::new(authority), Arc::new(hash)));
        match hash_type {
            // Needs EntryHash -> (creates, deletes, updates, entry)
            hash_type::AnyDht::Entry => {
                let mut ops = WireEntryOps::default();
                if let Some(el) = el {
                    // dbg!("FOUND");
                    ops.creates.push(Judged::valid(
                        el.signed_header().clone().try_into().unwrap(),
                    ));
                    ops.entry = Some(EntryData {
                        entry: el.entry().as_option().cloned().unwrap(),
                        entry_type: el.header().entry_type().unwrap().clone(),
                    });
                }
                WireOps::Entry(ops)
            }
            hash_type::AnyDht::Header => {
                let mut ops = WireElementOps::default();
                if let Some(el) = el {
                    // dbg!("FOUND");
                    ops.header = Some(Judged::valid(SignedHeader(
                        el.header().clone(),
                        el.signature().clone(),
                    )));
                    ops.entry = el.entry().as_option().cloned();
                }
                WireOps::Element(ops)
            }
        }
    }

    pub fn peer_query(&self, authority: &AgentPubKey, loc: DhtLocation) -> Vec<AgentInfoSigned> {
        let mut peers = self.all_held_agents.get(authority).unwrap().clone();
        peers
            .sort_unstable_by_key(|info| shortest_arc_distance(loc, info.storage_arc.center_loc()));
        peers.truncate(8);
        peers
    }

    pub fn peer_get(
        &self,
        authority: &AgentPubKey,
        agent: Arc<KitsuneAgent>,
    ) -> Option<AgentInfoSigned> {
        let info = self
            .agent_to_info
            .get(&AgentPubKey::from_kitsune(&agent))
            .cloned()?;
        self.agent_to_info
            .get(authority)
            .unwrap()
            .storage_arc
            .contains(info.storage_arc.center_loc())
            .then(|| info)
    }
}
