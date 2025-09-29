
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::net::UdpSocket;
use std::time::{Duration, Instant};
use chrono::Utc;
// use std::io::Seek;

const PAYLOAD_SIZE: usize = 1000;
const WINDOW_SIZE: usize = 256;
const RETRANS_TIMEOUT_MS: u64 = 250;
const SERVER_BIND: &'static str = "127.0.0.1:4000";
const CLIENT_ADDR: &'static str = "127.0.0.1:5000";

fn make_data_packet(stream_id: u16, seq: u32, timestamp: u64, payload: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(18 + payload.len());
    v.push(0x01u8);
    v.push(0u8);    // Flags
    v.extend_from_slice(&stream_id.to_be_bytes());
    v.extend_from_slice(&seq.to_be_bytes());
    v.extend_from_slice(&timestamp.to_be_bytes());
    v.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    v.extend_from_slice(payload);
    v
}

fn parse_ack(buf: &[u8]) -> Option<(u32, u64)> {
    // Expect at least 14 bytes: type(1)+res(1)+cumulative(4)+bitmap(8) = 14
    if buf.len() < 14  || buf[0] != 0x02 { return None; }
    let cumulative = u32::from_be_bytes([buf[2], buf[3], buf[4], buf[5]]);
    let bitmap = u64::from_be_bytes([
        buf[6],buf[7],buf[8],buf[9],buf[10],buf[11],buf[12],buf[13]
    ]);
    Some((cumulative, bitmap))
}

fn main() -> std::io::Result<()> {
    println!("Fluxel Server (prototype) starting...");
    let socket = UdpSocket::bind(SERVER_BIND)?;
    socket.connect(CLIENT_ADDR)?;
    socket.set_nonblocking(true)?;
    println!("Server bound {}, sending to {}", SERVER_BIND, CLIENT_ADDR);

    let mut file = File::open("video_test.bin")?;
    let mut seq_base: u32 = 0; // next seq to create
    let mut send_buffer: BTreeMap<u32, (Vec<u8>, Instant, usize)> = BTreeMap::new();
    let mut read_eof = false;
    let mut buf_recv = [0u8; 1500];

    loop {
        // Fill window by reading file and sending new packets
        while send_buffer.len() < WINDOW_SIZE && !read_eof {
            let mut payload = vec![0u8; PAYLOAD_SIZE];
            let n = file.read(&mut payload)?;
            if n == 0 {
                read_eof = true;
                break;
            }
            payload.truncate(n);
            let ts = Utc::now().timestamp_millis() as u64;
            let pkt = make_data_packet(1, seq_base, ts, &payload);
            socket.send(&pkt)?;
            send_buffer.insert(seq_base, (pkt, Instant::now(), 1));
            println!("SENT seq={}", seq_base);
            seq_base = seq_base.wrapping_add(1);
        }

        // receive ACKs (non-blocking)
        match socket.recv(&mut buf_recv) {
            Ok(r) => {
                if let Some((cumulative, bitmap)) = parse_ack(&buf_recv[..r]) {
                    println!("ACK cumulative={} bitmap={:064b}", cumulative, bitmap);
                    // Remove all <= cumulative
                    let keys: Vec<u32> = send_buffer.keys().copied().collect();
                    for k in keys {
                        if k <= cumulative {
                            send_buffer.remove(&k);
                        }
                    }
                    // For next 64 seq after cumulative, bitmap bit==1 means client has it;
                    // bit==0 -> missing -> retransmit if we have it.
                    for i in 0..64u32 {
                        let seq = cumulative.wrapping_add(1).wrapping_add(i);
                        let bit = (bitmap >> i) & 1u64;
                        if bit == 0 {
                            if let Some((pkt, _, _)) = send_buffer.get(&seq) {
                                // retransmit missing
                                let _ = socket.send(pkt);
                                println!("Retransmit (per bitmap) seq={}", seq);
                                // update send_time & attempts
                                if let Some(entry) = send_buffer.get_mut(&seq) {
                                    entry.1 = Instant::now();
                                    entry.2 += 1;
                                }
                            }
                        }
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => eprintln!("recv err: {:?}", e),
        }

        // retransmit by timeout
        let now = Instant::now();
        let mut to_retrans = Vec::new();
        for (&k, (pkt, sent_time, _attempts)) in send_buffer.iter() {
            if now.duration_since(*sent_time) > Duration::from_millis(RETRANS_TIMEOUT_MS) {
                to_retrans.push(k);
            }
        }
        for k in to_retrans {
            if let Some((pkt, sent_time, attempts)) = send_buffer.get_mut(&k) {
                let _ = socket.send(pkt);
                *sent_time = Instant::now();
                *attempts += 1;
                println!("Retrans timeout -> resent seq={} attempts={}", k, attempts);
            }
        }

        // termination: if EOF and send_buffer empty -> done
        if read_eof && send_buffer.is_empty() {
            println!("All sent & acked. Exiting server.");
            break;
        }

        std::thread::sleep(Duration::from_millis(10));
    }

    Ok(())
}


// use std::fs::File;
// use std::io::Read;
// use std::net::UdpSocket;

// fn main() {
//     let socket = UdpSocket::bind("127.0.0.1:4000").unwrap();

//     let mut file = File::open("video_test.txt").unwrap();
//     let mut buffer = [0u8; 512];
//     let mut seq_num: u32 = 0;

//     let client_addr = "127.0.0.1:5000";

//     loop {
//         let bytes_read = file.read(&mut buffer).unwrap();
//         if bytes_read == 0 {
//             break;
//         }

//         let mut packet = Vec::new();
//         packet.extend_from_slice(&seq_num.to_be_bytes());
//         packet.extend_from_slice(&(bytes_read as u16).to_be_bytes());
//         packet.extend_from_slice(&buffer[..bytes_read]);

//         socket.send_to(&packet, client_addr).unwrap();
//         println!("sent pkt seq={} ({bytes_read} bytes)", seq_num);

//         seq_num += 1;
//     }
// }
