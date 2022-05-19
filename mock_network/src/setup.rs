use std::sync::Arc;

use holochain_p2p::mock_network::{HolochainP2pMockChannel, MockScenario};
use holochain_p2p::AgentPubKeyExt;
use holochain_types::prelude::AgentPubKey;
use kitsune_p2p::{agent_store::AgentInfoSigned, KitsuneP2pConfig, TransportConfig};
use kitsune_p2p_proxy::ProxyUrl;
use kitsune_p2p_types::{tx2::tx2_adapter::AdapterFactory, Tx2Cert};

use crate::network::quic::Quic;

use super::*;

pub fn setup(peer_data: Vec<AgentInfoSigned>) -> (MockNetwork, KitsuneP2pConfig) {
    // Create the simulated network.
    let (from_kitsune_tx, to_kitsune_rx, channel) = HolochainP2pMockChannel::channel(
        // Pass in the generated simulated peer data.
        peer_data
            .into_iter()
            .map(|info| {
                let url = info.url_list.get(0).cloned().unwrap();
                let cert = Tx2Cert::from(ProxyUrl::from_full(url.as_str()).unwrap().digest());
                (info, cert)
            })
            .collect(),
        // We want to buffer up to 1000 network messages.
        1000,
        MockScenario {
            // Don't drop any individual messages.
            percent_drop_msg: 0.0,
            // A random 10% of simulated agents will never be online.
            percent_offline: 0.0,
            // Simulated agents will receive messages from within 50 to 100 ms.
            inbound_delay_range: std::time::Duration::from_millis(50)
                ..std::time::Duration::from_millis(150),
            // Simulated agents will send messages from within 50 to 100 ms.
            outbound_delay_range: std::time::Duration::from_millis(50)
                ..std::time::Duration::from_millis(150),
        },
    );
    let mock_network =
        kitsune_p2p::test_util::mock_network::mock_network(from_kitsune_tx, to_kitsune_rx);
    let mock_network: AdapterFactory = Arc::new(mock_network);

    // Setup the network.
    let mut tuning =
        kitsune_p2p_types::config::tuning_params_struct::KitsuneP2pTuningParams::default();
    tuning.gossip_strategy = "sharded-gossip".to_string();
    tuning.gossip_dynamic_arcs = true;

    let mut network = KitsuneP2pConfig::default();
    network.transport_pool = vec![TransportConfig::Mock {
        mock_network: mock_network.into(),
    }];
    network.tuning_params = Arc::new(tuning);
    (MockNetwork::new(channel), network)
}

pub fn real_networked(peer_data: Arc<Vec<AgentInfoSigned>>, quic: Quic) -> MockNetwork {
    // Create the simulated network.
    let (from_kitsune_tx, to_kitsune_rx, channel) = HolochainP2pMockChannel::channel(
        // Pass in the generated simulated peer data.
        peer_data
            .iter()
            .map(|info| {
                let cert = quic
                    .agents
                    .get(&AgentPubKey::from_kitsune(&info.agent))
                    .unwrap()
                    .handle()
                    .local_cert();
                (info.clone(), cert)
            })
            .collect(),
        // We want to buffer up to 1000 network messages.
        1000,
        MockScenario {
            // Don't drop any individual messages.
            percent_drop_msg: 0.0,
            // A random 10% of simulated agents will never be online.
            percent_offline: 0.0,
            // Simulated agents will receive messages from within 50 to 100 ms.
            inbound_delay_range: std::time::Duration::from_millis(50)
                ..std::time::Duration::from_millis(150),
            // Simulated agents will send messages from within 50 to 100 ms.
            outbound_delay_range: std::time::Duration::from_millis(50)
                ..std::time::Duration::from_millis(150),
        },
    );

    quic.spawn(from_kitsune_tx, to_kitsune_rx);

    MockNetwork::new(channel)
}
