use std::net::UdpSocket;
use std::chrono::Utc;
use std::time::{Duration, Instant};
use std::fs::File;
use std::collections::BTreeMap;
use std::time::Instant;
use std::io::Read;

use chrono::Utc;

const SERVER_BIND: &'static str = "127.0.0.1:4000";
const CLIENT_ADDR: &'static str = "127.0.0.1:50000";
const WINDOW_SIZE: usize = 256 ;
const PAYLOAD_SIZE: usize = 1000;

//type(1) + flags(1) + stream_id(2) + seq(4) + timestamp(8) + len_packet(2) + payload(len_packet)
fn make_data_packet(stream_id: u16, seq: u32, timestamp: u64, payload: &[u8]) -> Vec<u8> {
    let mut p: Vec<u8> = Vec::with_capacity(18 + payload.len());
    p.push(0x01u8);
    p.push(0u8);
    p.extend_from_slice(&stream_id.to_be_bytes());
    p.extend_from_slice(&seq.to_be_bytes());
    p.extend_from_slice(&timestamp.to_be_bytes());
    p.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    p.extend_from_slice(payload);
    p
}

// type(1) + flag(1) + cummulative(4) + bitmap(8)
fn parse_ack(buffer: &[u8]) -> Option<(u32, u64)> {
    if buffer.len() < 14 || buffer[0] != 0x02 {
        return None;
    }
    let cumulative = u32::from_be_bytes([buffer[2], buffer[3], buffer[4], buffer[5]]);
    let bitmap = u64::from_be_bytes([
        buffer[6],buffer[7],buffer[8],buffer[9],buffer[10],buffer[11],buffer[12],buffer[13]
    ]);
    return Some((cumulative, bitmap))

}

fn log(level: &str, source: &str, message: &str) {
    let now = chrono::Utc::now().to_rfc3339();
    println!("[{}] [{:5}] [{}] {}", now, level, source, message);
}


fn main() {
    log("INFO", "fluxel", "Fluxel Server starting...");
    let socket = UdpSocket::bind(SERVER_BIND)?;
    socket.connect(CLIENT_ADDR)?;
    socket.set_nonblocking(true)?;
    log("INFO", "udp", &format!("Server bound {}, sending to {}", SERVER_BIND, CLIENT_ADDR));

    let mut file = File::open("video_text.txt");
    let mut seq_base: u32 = 0;
    let mut send_buffer: BTreeMap<u32, (Vec<u8>, Instant, usize)> = BTreeMap::new();
    let mut read_eof = false;
    let mut buf_recv = [0u8; 1500];

    loop {
        while buf_recv.len() < WINDOW_SIZE || !read_eof {
            let mut payload = vec![0u8; PAYLOAD_SIZE];
            let n = file.read(&mut payload);
            if n==0 {
                read_eof = true;
                break;
            }
            payload.truncate(n);
            let timestamp = Utc::now().timestamp_millis() as u64;
            let packet = make_data_packet(1, seq_base, timestamp, &payload);
            socket.send(&packet)?;
            send_buffer.insert(seq_base, (packet, Instant::now(), 1));
            log("DEBUG", "send", &format!("SENT seq={}", seq_base));
            seq_base = seq_base.wrapping_add(1);
        }
    }
} 