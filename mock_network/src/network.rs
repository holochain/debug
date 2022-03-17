use std::sync::Arc;

use holochain_p2p::mock_network::*;
use holochain_p2p::AgentPubKeyExt;
use holochain_p2p::WireMessage;
use holochain_types::dht_op::WireOps;
use holochain_types::prelude::*;
use kitsune_p2p::agent_store::AgentInfoSigned;
use kitsune_p2p::gossip::sharded_gossip::ShardedGossipWire;
use kitsune_p2p::test_util::mock_network::KitsuneMockAddr;
use kitsune_p2p::wire as kwire;
use kitsune_p2p_proxy::ProxyUrl;
use kitsune_p2p_types::Tx2Cert;
use observability::tracing;

pub mod quic;

pub struct MockNetwork {
    mock: HolochainP2pMockChannel,
}

impl MockNetwork {
    pub(crate) fn new(mock: HolochainP2pMockChannel) -> Self {
        Self { mock }
    }

    pub fn add_real_agents(&mut self, peer_data: Vec<AgentInfoSigned>) {
        let iter = peer_data.into_iter().map(|a| {
            let agent = Arc::new(AgentPubKey::from_kitsune(&a.agent));
            let url = a.url_list.get(0).cloned().unwrap();
            let cert = Tx2Cert::from(ProxyUrl::from_full(url.as_str()).unwrap().digest());
            (agent, Arc::new(KitsuneMockAddr { cert, url }))
        });
        self.mock.add_real_agents(iter);
    }

    pub async fn next(
        &mut self,
    ) -> Option<(
        AddressedHolochainP2pMockMsg,
        Option<HolochainP2pMockRespond>,
    )> {
        self.mock.next().await
    }

    pub async fn send(&self, msg: AddressedHolochainP2pMockMsg) -> Option<HolochainP2pMockMsg> {
        self.mock.send(msg).await
    }

    /// Get a real nodes address.
    pub fn real_node_address(&self, agent: &Agent) -> Arc<KitsuneMockAddr> {
        self.mock.real_node_address(agent)
    }

    #[tracing::instrument(skip(self, msg, response))]
    pub async fn default_response(
        &self,
        msg: AddressedHolochainP2pMockMsg,
        response: Option<HolochainP2pMockRespond>,
    ) {
        let AddressedHolochainP2pMockMsg {
            msg,
            to_agent,
            from_agent,
        } = msg;
        match msg {
            HolochainP2pMockMsg::Wire { msg, .. } => match msg {
                WireMessage::Get { dht_hash, .. } => {
                    let r = match dht_hash.hash_type() {
                        hash_type::AnyDht::Entry => WireOps::Entry(Default::default()),
                        hash_type::AnyDht::Header => WireOps::Element(Default::default()),
                    };
                    let r: Vec<u8> =
                        UnsafeBytes::from(SerializedBytes::try_from(r).unwrap()).into();
                    let msg = HolochainP2pMockMsg::CallResp(r.into());
                    response.unwrap().respond(msg);
                }
                WireMessage::GetLinks { .. } => {
                    let r: Vec<u8> = UnsafeBytes::from(
                        SerializedBytes::try_from(WireLinkOps::default()).unwrap(),
                    )
                    .into();
                    let msg = HolochainP2pMockMsg::CallResp(r.into());
                    response.unwrap().respond(msg);
                }
                WireMessage::GetAgentActivity { agent, .. } => {
                    let msg: AgentActivityResponse<HeaderHash> = AgentActivityResponse {
                        agent,
                        valid_activity: ChainItems::Hashes(Default::default()),
                        rejected_activity: ChainItems::Hashes(Default::default()),
                        status: ChainStatus::Empty,
                        highest_observed: None,
                    };
                    let r: Vec<u8> =
                        UnsafeBytes::from(SerializedBytes::try_from(msg).unwrap()).into();
                    let msg = HolochainP2pMockMsg::CallResp(r.into());
                    response.unwrap().respond(msg);
                }
                WireMessage::ValidationReceipt { .. } => {
                    let msg = HolochainP2pMockMsg::CallResp(Vec::with_capacity(0).into());
                    response.unwrap().respond(msg);
                }
                WireMessage::CallRemote { .. }
                | WireMessage::GetMeta { .. }
                | WireMessage::GetValidationPackage { .. } => response.unwrap().respond(
                    HolochainP2pMockMsg::Failure("Not accepting this request".to_string()),
                ),
                _ => (),
            },
            HolochainP2pMockMsg::Gossip { dna, module, .. } => {
                let msg = HolochainP2pMockMsg::Gossip {
                    dna,
                    module,
                    gossip: GossipProtocol::Sharded(ShardedGossipWire::error(
                        "Not accepting gossip".to_string(),
                    )),
                };
                self.mock
                    .send(msg.addressed(to_agent.clone(), from_agent.clone()))
                    .await;
            }
            HolochainP2pMockMsg::CallResp(_) => {
                tracing::debug!("Received call response from {}", from_agent)
            }
            HolochainP2pMockMsg::MetricExchange(_) => (),
            HolochainP2pMockMsg::PeerGet(_) => {
                response
                    .unwrap()
                    .respond(HolochainP2pMockMsg::Failure("Missing peer".to_string()));
            }
            HolochainP2pMockMsg::PeerGetResp(_) => (),
            HolochainP2pMockMsg::PeerQuery(_) => {
                response
                    .unwrap()
                    .respond(HolochainP2pMockMsg::PeerQueryResp(kwire::PeerQueryResp {
                        peer_list: vec![],
                    }))
            }
            HolochainP2pMockMsg::PeerQueryResp(_) => (),
            HolochainP2pMockMsg::PublishedAgentInfo {
                to_agent,
                dna,
                info,
            } => tracing::debug!(
                "Publish agent info sent to {} for space {} with info {:?}",
                to_agent,
                dna,
                info
            ),
            HolochainP2pMockMsg::Failure(error) => {
                tracing::error!("Received failure {} from {}", error, from_agent)
            }
        }
    }
}
