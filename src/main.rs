use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::Write;
use std::net::UdpSocket;
use std::time::{Duration, Instant};

const CLIENT_BIND: &'static str = "127.0.0.1:50000";
const SERVER_ADDR: &'static str = "127.0.0.1:4000";
// const PAYLOAD_SIZE: usize = 1000;
const ACK_INTERVAL_MS: u64 = 100;


// type(1) + flags(1) + stream_id(2) + seq(4) + timestamp(8) + len(2) + payload(len)
fn parse_data_packet(buf: &[u8]) -> Option<(u32, Vec<u8>)> {
    if buf.len() < 18 || buf[0] != 0x01 {
        return None;
    }
    let seq = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
    let len = u16::from_be_bytes([buf[16], buf[17]]) as usize;
    if buf.len() < 18 + len {
        return None;
    }
    let payload = buf[18..18 + len].to_vec();
    Some((seq, payload))
}

fn make_ack_packet(cumulative: u32, bitmap: u64) -> Vec<u8> {
    let mut v = Vec::with_capacity(14);
    v.push(0x02u8); 
    v.push(0u8);    
    v.extend_from_slice(&cumulative.to_be_bytes());
    v.extend_from_slice(&bitmap.to_be_bytes());
    v
}

fn log(level: &str, source: &str, message: &str) {
    let now = chrono::Utc::now().to_rfc3339();
    println!("[{}] [{:5}] [{}] {}", now, level, source, message);
}

fn main() -> std::io::Result<()> {
    log("INFO", "fluxel", "Fluxel Client starting...");

    let socket = UdpSocket::bind(CLIENT_BIND)?;
    socket.connect(SERVER_ADDR)?;
    socket.set_nonblocking(true)?;

    log("INFO", "udp", &format!("Client bound {}, receiving from {}", CLIENT_BIND, SERVER_ADDR));

    let mut received: BTreeMap<u32, Vec<u8>> = BTreeMap::new();
    let mut received_set: HashSet<u32> = HashSet::new();
    let mut cumulative_ack: u32 = 0;
    let mut file = File::create("received_output.txt")?;
    let mut buf = [0u8; 1500];

    let mut last_ack = Instant::now();

    loop {
        match socket.recv(&mut buf) {
            Ok(n) => {
                if let Some((seq, payload)) = parse_data_packet(&buf[..n]) {
                    if received_set.insert(seq) {
                        received.insert(seq, payload);
                        log("DEBUG", "recv", &format!("Received seq={}", seq));
                    } else {
                        log("DEBUG", "dup", &format!("Duplicate seq={}", seq));
                    }

                    while let Some(data) = received.remove(&cumulative_ack) {
                        file.write_all(&data)?;
                        cumulative_ack = cumulative_ack.wrapping_add(1);
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => log("ERROR", "recv", &format!("Receive error: {:?}", e)),
        }

        if last_ack.elapsed() > Duration::from_millis(ACK_INTERVAL_MS) {
            let mut bitmap: u64 = 0;
            for i in 0..64 {
                let seq = cumulative_ack.wrapping_add(1 + i);
                if received_set.contains(&seq) {
                    bitmap |= 1 << i;
                }
            }

            let ack_packet = make_ack_packet(cumulative_ack.wrapping_sub(1), bitmap);
            socket.send(&ack_packet)?;
            log("INFO", "ack", &format!("Sent ACK: cumulative={}, bitmap={:064b}", cumulative_ack.wrapping_sub(1), bitmap));

            last_ack = Instant::now();
        }

        std::thread::sleep(Duration::from_millis(5));

    }
}
