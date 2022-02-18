use std::sync::Arc;

use holochain_p2p::{AgentPubKeyExt, DnaHashExt};
use kitsune_p2p::{
    agent_store::AgentInfoSigned, dependencies::kitsune_p2p_proxy::ProxyUrl, KitsuneSignature,
};
use kitsune_p2p_types::{tls::TlsConfig, tx2::tx2_utils::TxUrl};

use super::*;

pub struct GenerateAgentInfo<'a, 'b, I>
where
    I: IntoIterator<Item = &'b AgentPubKey>,
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
}

impl<'a, 'b, I> GenerateAgentInfo<'a, 'b, I>
where
    I: IntoIterator<Item = &'b AgentPubKey>,
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
            async move {
                let tls = TlsConfig::new_ephemeral().await.unwrap();
                let url: TxUrl = ProxyUrl::new("kitsune-quic://localhost:5778", tls.cert_digest)
                    .unwrap()
                    .as_str()
                    .into();
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
                            Ok(
                                holochain_keystore::AgentPubKeyExt::sign(agent, keystore, bytes)
                                    .await
                                    .map(|s| Arc::new(KitsuneSignature(s.0.to_vec())))
                                    .unwrap(),
                            )
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
