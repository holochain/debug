use std::sync::Arc;

use holochain_p2p::{dht_arc::DhtArc, AgentPubKeyExt, DnaHashExt};
use kitsune_p2p::{
    agent_store::AgentInfoSigned, dependencies::kitsune_p2p_proxy::ProxyUrl, KitsuneSignature,
};
use kitsune_p2p_types::{tls::TlsConfig, tx2::tx2_utils::TxUrl};

use crate::network::quic::Quic;

use super::*;

pub struct GenerateQuicAgents<'a, 'b, I>
where
    I: IntoIterator<Item = &'b Arc<AgentPubKey>>,
{
    pub agents: GenerateAgentInfo<'a, 'b, I>,
}

pub struct GenerateAgentInfo<'a, 'b, I>
where
    I: IntoIterator<Item = &'b Arc<AgentPubKey>>,
{
    pub keystore: &'a MetaLairClient,
    pub agent_keys: I,
    pub dna_hash: DnaHash,
    pub settings: SettingsBuilder,
}

#[derive(Builder)]
pub struct Settings {
    #[builder(default = "u32::MAX / 2")]
    pub dht_storage_arc_half_length: u32,
    #[builder(default = "SystemTime::now()")]
    pub signed_at: SystemTime,
    #[builder(default = "SystemTime::now() + Duration::from_secs(60 * 60)")]
    pub expires_at: SystemTime,
    #[builder(default)]
    pub urls: HashMap<Arc<AgentPubKey>, TxUrl>,
}

impl<'a, 'b, I> GenerateQuicAgents<'a, 'b, I>
where
    I: IntoIterator<Item = &'b Arc<AgentPubKey>>,
{
    pub async fn make(self) -> (Vec<AgentInfoSigned>, Quic) {
        let Self { agents } = self;
        let keys = agents.agent_keys.into_iter().collect::<Vec<_>>();
        let quic = Quic::new(keys.iter().map(|n| *n)).await;
        let mut agents = GenerateAgentInfo {
            agent_keys: keys,
            keystore: agents.keystore,
            dna_hash: agents.dna_hash,
            settings: agents.settings,
        };
        agents.settings.urls(quic.get_urls().collect());
        let peer_data = agents.make().await;
        (peer_data, quic)
    }
}

impl<'a, 'b, I> GenerateAgentInfo<'a, 'b, I>
where
    I: IntoIterator<Item = &'b Arc<AgentPubKey>>,
{
    pub async fn make(self) -> Vec<AgentInfoSigned> {
        let Self {
            keystore,
            agent_keys,
            dna_hash,
            settings,
        } = self;
        let Settings {
            dht_storage_arc_half_length,
            signed_at,
            expires_at,
            urls,
        } = settings.build().unwrap();
        let signed_at_ms = signed_at
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let expires_at_ms = expires_at
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let stream = agent_keys.into_iter().map(|agent| {
            let agent_kit = agent.to_kitsune();
            let space = dna_hash.to_kitsune();
            let url = urls.get(agent).cloned();
            async move {
                let url: TxUrl = match url {
                    Some(url) => url,
                    None => {
                        let tls = TlsConfig::new_ephemeral().await.unwrap();

                        ProxyUrl::new("kitsune-quic://localhost:5778", tls.cert_digest.clone())
                            .unwrap()
                            .as_str()
                            .into()
                    }
                };
                AgentInfoSigned::sign(
                    space,
                    agent_kit,
                    dht_storage_arc_half_length,
                    vec![url],
                    signed_at_ms,
                    expires_at_ms,
                    |bytes| {
                        let bytes = bytes.to_vec();
                        async move {
                            Ok(holochain_keystore::AgentPubKeyExt::sign(
                                agent.as_ref(),
                                keystore,
                                bytes,
                            )
                            .await
                            .map(|s| Arc::new(KitsuneSignature(s.0.to_vec())))
                            .unwrap())
                        }
                    },
                )
                .await
                .unwrap()
            }
        });
        futures::stream::iter(stream)
            .buffer_unordered(10)
            .collect()
            .await
    }
}

pub fn ideal_target(total_peers: usize, arc: DhtArc) -> DhtArc {
    let coverage = 50.0 / total_peers as f64;
    DhtArc::with_coverage(arc.center_loc(), coverage)
}
