use std::path::PathBuf;

use kitsune_p2p::dht_arc::DhtArc;

use super::prelude::*;
#[derive(Debug, Serialize, Deserialize, SerializedBytes)]
pub enum Msg {
    Hashes(Vec<HeaderHash>),
    DhtArc(DhtArc),
    DbSent(PathBuf),
    NoDb,
}
