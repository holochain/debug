use std::path::PathBuf;

use kitsune_p2p::agent_store::AgentInfoSigned;
use kitsune_p2p::dht_arc::DhtArc;

use super::prelude::*;
#[derive(Debug, Serialize, Deserialize, SerializedBytes)]
pub enum Msg {
    Hashes(Vec<HeaderHash>),
    Infos(Vec<AgentInfoSigned>),
    DhtArc(DhtArc),
    DbSent(PathBuf),
    NoDb,
}
