use serde::{Serialize, Deserialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Debug)]
struct FluxelPacket {
    seq_num: u32,
    timestamp: u128,
    payload: String,
}

impl FluxelPacket {
    fn new(seq_num: u32, payload: String) -> Self {
        Self {
            seq_num,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millise(),
            payload,
        }
    }
}