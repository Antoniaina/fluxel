use std::fs::File;
use std::io::Read;
use std::net::UdpSocket;

fn main() {
    let socket = UdpSocket::bind("127.0.0.1:4000").unwrap();

    let mut file = File::open("video_test.txt").unwrap();
    let mut buffer = [0u8; 512];
    let mut seq_num: u32 = 0;

    let client_addr = "127.0.0.1:5000";

    loop {
        let bytes_read = file.read(&mut buffer).unwrap();
        if bytes_read == 0 {
            break;
        }

        let mut packet = Vec::new();
        packet.extend_from_slice(&seq_num.to_be_bytes());
        packet.extend_from_slice(&(bytes_read as u16).to_be_bytes());
        packet.extend_from_slice(&buffer[..bytes_read]);

        socket.send_to(&packet, client_addr).unwrap();
        println!("sent pkt seq={} ({bytes_read} bytes)", seq_num);

        seq_num += 1;
    }
}
