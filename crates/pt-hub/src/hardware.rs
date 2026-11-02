use std::sync::Arc;
use std::thread;
use std::time::Duration;

use pt_api::HubStreamMessage;

use crate::hub::Hub;
use crate::log::warning;
use crate::transcript::lock;

pub fn start(hub: &Arc<Hub>) {
    let cadence = pt_fleet::PollConfig::default();
    let inventory_every = cadence.inventory_every;
    let sample_every = sample_interval(cadence.sample_every);
    let weak = Arc::downgrade(hub);
    let inventory = thread::Builder::new()
        .name("pt-hub-inventory".into())
        .spawn(move || {
            loop {
                let inventory = pt_fleet::view::inventory(&pt_node::inventory::collect());
                let Some(hub) = weak.upgrade() else { return };
                let mut info = lock(&hub.info);
                info.inventory = Some(inventory);
                hub.bus.push(|seq| HubStreamMessage::Hub {
                    seq,
                    hub: info.clone(),
                });
                drop(info);
                drop(hub);
                thread::sleep(inventory_every);
            }
        });
    if let Err(e) = inventory {
        warning!("hub inventory: {e}");
    }

    let weak = Arc::downgrade(hub);
    let sample = thread::Builder::new()
        .name("pt-hub-sample".into())
        .spawn(move || {
            let mut sampler = pt_node::Sampler::new();
            loop {
                let sample = pt_fleet::view::sample(&sampler.sample());
                let Some(hub) = weak.upgrade() else { return };
                let mut info = lock(&hub.info);
                info.sample = Some(sample);
                hub.bus.push(|seq| HubStreamMessage::Hub {
                    seq,
                    hub: info.clone(),
                });
                drop(info);
                drop(hub);
                thread::sleep(sample_every);
            }
        });
    if let Err(e) = sample {
        warning!("hub sample: {e}");
    }
}

fn sample_interval(default: Duration) -> Duration {
    if let Some(ms) = std::env::var("PT_TEST_HUB_SAMPLE_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|&ms| ms >= 1000)
    {
        return Duration::from_millis(ms);
    }
    default
}
