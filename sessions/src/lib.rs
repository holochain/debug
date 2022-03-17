use std::net::SocketAddr;

pub mod prelude {
    pub use holo_hash::*;
    pub use holochain::conductor::config::ConductorConfig;
    pub use holochain::conductor::ConductorBuilder;
    pub use holochain::sweettest::*;
    pub use holochain_cascade::*;
    pub use holochain_state::test_utils::*;
    pub use holochain_types::*;
    pub use holochain_zome_types::prelude::*;
    pub use holochain_zome_types::SerializedBytes;
    pub use kitsune_p2p::KitsuneP2pConfig;
    pub use kitsune_p2p::TransportConfig;
    pub use kitsune_p2p_types::tx2::tx2_adapter::AdapterFactory;
    pub use serde::Deserialize;
    pub use serde::Serialize;
}

pub mod build;
pub mod perf;
pub mod shared_types;

pub async fn run_bootstrap() -> SocketAddr {
    match kitsune_p2p_bootstrap::run(
        (dbg!(controller::lan_ip()), 0),
        // TODO - get from argv
        vec![],
    )
    .await
    {
        Ok((driver, addr)) => {
            println!("http://{}", addr);
            tokio::spawn(driver);
            addr
        }
        Err(err) => panic!("{}", err),
    }
}
