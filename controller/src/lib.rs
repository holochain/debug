pub mod bootstrap;
pub mod connect;
pub mod remote;
pub mod local;

use std::net::IpAddr;

pub fn lan_ip() -> IpAddr {
    local_ip_address::list_afinet_netifas()
        .unwrap()
        .into_iter()
        .find_map(|(_, addr)| match addr {
            IpAddr::V4(i) => {
                if i.octets()[0] == 192 {
                    Some(IpAddr::V4(i))
                } else {
                    None
                }
            }
            _ => None,
        })
        .unwrap()
}
