use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use chrono::Utc;

const CLIENT_BIND: &str = "127.0.0.1:5000";
const SERVER_ADDR: &str = "127.0.0.1:4000";
const ACK_INTERVAL_MS: u64 = 80;
const PLAYOUT_DELAY_MS: u64 = 150;
const ACK_BITMAP_SIZE: usize = 64;

fn build_ack(cumulative: u32, bitmap: u64) -> Vec<u8> {
    let mut p = Vec::with_capacity(14);
    p.push(0x02u8); 
    p.push(0u8);   
    p.extend_from_slice(&cumulative.to_be_bytes());
    p.extend_from_slice(&bitmap.to_be_bytes());
    p
}

fn main() -> std::io::Result<()> {
    println!("Fluxel Client (prototype) starting...");
    let socket = UdpSocket::bind(CLIENT_BIND)?;
    socket.connect(SERVER_ADDR)?;
    socket.set_nonblocking(true)?;
    println!("Client bound {}, sending ACKs to {}", CLIENT_BIND, SERVER_ADDR);

    // shared recv buffer: seq -> (payload, timestamp_received)
    let shared_buf: Arc<Mutex<BTreeMap<u32, (Vec<u8>, u64)>>> = Arc::new(Mutex::new(BTreeMap::new()));
    let shared_expected: Arc<Mutex<u32>> = Arc::new(Mutex::new(0u32));

    {
        let buf_clone = Arc::clone(&shared_buf);
        let exp_clone = Arc::clone(&shared_expected);
        thread::spawn(move || {
            let mut out = File::create("reception_playout.bin").expect("create file");
            loop {
                let mut expected = { *exp_clone.lock().unwrap() };
                loop {
                    let mut removed = None;
                    {
                        let mut guard = buf_clone.lock().unwrap();
                        if let Some((payload, ts)) = guard.get(&expected) {
                            let now_ms = Utc::now().timestamp_millis() as u64;
                            if now_ms >= *ts + PLAYOUT_DELAY_MS {
                                if let Err(e) = out.write_all(payload) {
                                    eprintln!("write err: {:?}", e);
                                }
                                removed = Some(expected);
                            }
                        }
                    }
                    if let Some(seq) = removed {
                        let mut guard = buf_clone.lock().unwrap();
                        guard.remove(&seq);
                        expected = expected.wrapping_add(1);
                        *exp_clone.lock().unwrap() = expected;
                        continue;
                    } else {
                        break;
                    }
                }
                thread::sleep(Duration::from_millis(10));
            }
        });
    }

    let recv_buf = Arc::clone(&shared_buf);
    let expected_seq = Arc::clone(&shared_expected);

    // Receiver + ACK sender loop
    let mut buf = [0u8; 1500];
    let mut last_ack_time = Instant::now();

    loop {
        // recv non-blocking
        match socket.recv(&mut buf) {
            Ok(r) => {
                if r < 18 { continue; }
                if buf[0] != 0x01 { continue; } // only DATA packets here
                let stream_id = u16::from_be_bytes([buf[2], buf[3]]);
                let seq = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
                let timestamp = u64::from_be_bytes([
                    buf[8],buf[9],buf[10],buf[11],buf[12],buf[13],buf[14],buf[15]
                ]);
                let len = u16::from_be_bytes([buf[16], buf[17]]) as usize;
                let payload = buf[18..18+len].to_vec();

                {
                    let mut g = recv_buf.lock().unwrap();
                    // insert only if absent
                    g.entry(seq).or_insert((payload, Utc::now().timestamp_millis() as u64));
                }
                println!("RECV seq={} stream={} ts={}", seq, stream_id, timestamp);
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => eprintln!("recv err: {:?}", e),
        }

        if last_ack_time.elapsed() > Duration::from_millis(ACK_INTERVAL_MS) {
            let mut cumulative = {
                let g = recv_buf.lock().unwrap();
                let mut expected = *expected_seq.lock().unwrap();
                while g.contains_key(&expected) {
                    expected = expected.wrapping_add(1);
                }
                expected.wrapping_sub(1)
            };

            // build bitmap for next 64 seqs after cumulative
            let mut bitmap: u64 = 0;
            {
                let g = recv_buf.lock().unwrap();
                for i in 0..ACK_BITMAP_SIZE {
                    let s = cumulative.wrapping_add(1).wrapping_add(i as u32);
                    if g.contains_key(&s) {
                        bitmap |= 1u64 << i;
                    }
                }
            }

            let ack = build_ack(cumulative, bitmap);
            let _ = socket.send(&ack);
            println!("SENT ACK cumulative={} bitmap={:064b}", cumulative, bitmap);
            last_ack_time = Instant::now();
        }

        std::thread::sleep(Duration::from_millis(5));
    }
}
