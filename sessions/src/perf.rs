use std::fmt::Write;

use holochain::tracing::warn;
use systemstat::{Platform, System};
pub struct Perf(System);

impl Perf {
    pub fn new() -> Self {
        let sys = System::new();
        Self(sys)
    }

    pub fn print_info(&self) {
        let mut s = String::new();
        let la = self.0.load_average().unwrap();
        writeln!(&mut s, "CPU: {:?}", la).unwrap();
        let mem = self.0.memory().unwrap();
        writeln!(
            &mut s,
            "Mem: {:.2}%",
            mem.free.0 as f64 / mem.total.0 as f64 * 100.0
        )
        .unwrap();
        for n in self.0.networks().unwrap().values() {
            if n.name == "lo" {
                let net = self.0.network_stats(&n.name).unwrap();
                writeln!(&mut s, "Net: {}: {:?}", n.name, net).unwrap();
            }
        }
        for (_, device) in self.0.block_device_statistics().unwrap() {
            if device.name == "nvme0n1" {
                writeln!(
                    &mut s,
                    "disk: {}: read {}ms, write {}ms, io {}ms, waiting {}ms",
                    device.name,
                    device.read_ticks,
                    device.write_ticks,
                    device.io_ticks,
                    device.time_in_queue
                )
                .unwrap();
            }
        }
        warn!("PERF: {}", s);
    }
}
