//! Demo relay: accepts tunnel sessions over UDP, decrypts the IP packets inside, logs them, and
//! answers pings to the tunnel gateway 10.66.0.1. It forwards nothing to the internet; a real
//! deployment would write the packets to a TUN device and NAT them.
//!
//!   relay keygen <dir>          write <dir>/server.key and <dir>/server.pub (base64)
//!   relay serve <dir> <addr>    listen on <addr>, for example 0.0.0.0:51820

use std::collections::hash_map::{Entry, HashMap};
use std::net::{Ipv4Addr, UdpSocket};
use std::path::Path;
use std::{env, fs, process};

use base64::{engine::general_purpose::STANDARD, Engine};
use tunnel_core::{ServerKeys, Session, INIT};

const GATEWAY: Ipv4Addr = Ipv4Addr::new(10, 66, 0, 1);

fn main() {
    let args: Vec<String> = env::args().collect();
    let result = match args.get(1).map(String::as_str) {
        Some("keygen") if args.len() == 3 => keygen(Path::new(&args[2])),
        Some("serve") if args.len() == 4 => load(Path::new(&args[2])).and_then(|keys| {
            let socket = UdpSocket::bind(&args[3]).map_err(|e| format!("bind {}: {e}", args[3]))?;
            println!("listening on {}", socket.local_addr().unwrap());
            serve(&keys, &socket);
            Ok(())
        }),
        _ => Err("usage: relay keygen <dir> | relay serve <dir> <addr>".into()),
    };
    if let Err(e) = result {
        eprintln!("{e}");
        process::exit(1);
    }
}

fn keygen(dir: &Path) -> Result<(), String> {
    let keys = ServerKeys::generate();
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    write_private(&dir.join("server.key"), &keys.to_bytes())?;
    fs::write(dir.join("server.pub"), STANDARD.encode(keys.public())).map_err(|e| e.to_string())?;
    println!("wrote {0}/server.key and {0}/server.pub", dir.display());
    Ok(())
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true) // never overwrite a key clients have already pinned
        .mode(0o600)
        .open(path)
        .and_then(|mut f| f.write_all(bytes))
        .map_err(|e| format!("{}: {e}", path.display()))
}

fn load(dir: &Path) -> Result<ServerKeys, String> {
    let bytes = fs::read(dir.join("server.key")).map_err(|e| format!("server.key: {e}"))?;
    ServerKeys::from_bytes(&bytes).map_err(|e| format!("server.key: {e:?}"))
}

fn serve(keys: &ServerKeys, socket: &UdpSocket) {
    // shortcut: sessions are never expired, fine for a demo; a real relay drops idle ones.
    // Each session keeps the ACK it was opened with, to answer the client's INIT retries.
    let mut sessions: HashMap<[u8; 8], (Session, Vec<u8>)> = HashMap::new();
    let mut buf = [0u8; 65535];
    loop {
        let Ok((n, from)) = socket.recv_from(&mut buf) else { continue };
        let packet = &buf[..n];
        if packet.first() == Some(&INIT) {
            match keys.accept(packet) {
                // A repeated INIT (a client retry, or an attacker replaying a recorded one) gets the
                // original ACK. A new session would start its replay window empty again.
                Ok((session, ack)) => {
                    let (_, ack) = match sessions.entry(session.id) {
                        Entry::Occupied(slot) => slot.into_mut(),
                        Entry::Vacant(slot) => {
                            println!("{from}: new session {}", hex(&session.id));
                            slot.insert((session, ack))
                        }
                    };
                    let _ = socket.send_to(ack, from);
                }
                Err(e) => println!("{from}: bad handshake {e:?}"),
            }
            continue;
        }
        let Some((session, _)) = tunnel_core::session_id(packet).and_then(|id| sessions.get_mut(&id)) else {
            continue;
        };
        let Ok(ip) = session.open(packet) else {
            println!("{from}: dropped unauthentic or replayed packet");
            continue;
        };
        println!("{from}: {}", describe(&ip));
        // Replies go to the latest source address, so a phone that switches from Wi-Fi to
        // mobile data keeps its session.
        if let Some(reply) = echo_reply(&ip) {
            let _ = socket.send_to(&session.seal(&reply), from);
        }
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn describe(ip: &[u8]) -> String {
    if ip.len() < 20 || ip[0] >> 4 != 4 {
        return format!("non-IPv4 packet, {} bytes", ip.len());
    }
    let proto = match ip[9] {
        1 => "ICMP",
        6 => "TCP",
        17 => "UDP",
        _ => "other",
    };
    let src = Ipv4Addr::new(ip[12], ip[13], ip[14], ip[15]);
    let dst = Ipv4Addr::new(ip[16], ip[17], ip[18], ip[19]);
    format!("{proto} {src} -> {dst}, {} bytes", ip.len())
}

/// Builds the reply to an ICMP echo request addressed to the gateway, or None for anything else.
fn echo_reply(ip: &[u8]) -> Option<Vec<u8>> {
    let ihl = usize::from(ip.first()? & 0x0f) * 4;
    // The header length comes from the packet itself; below 20 the fixed fields read past the end.
    if ihl < 20 || ip.len() < ihl + 8 || ip[0] >> 4 != 4 || ip[9] != 1 || ip[16..20] != GATEWAY.octets() || ip[ihl] != 8 {
        return None;
    }
    let mut r = ip.to_vec();
    r.copy_within(12..16, 16); // destination = old source
    r[12..16].copy_from_slice(&GATEWAY.octets());
    r[8] = 64; // TTL
    r[ihl] = 0; // echo reply
    set_checksum(&mut r, 10, 0..ihl);
    set_checksum(&mut r, ihl + 2, ihl..ip.len());
    Some(r)
}

fn set_checksum(p: &mut [u8], at: usize, range: std::ops::Range<usize>) {
    p[at..at + 2].fill(0);
    let sum = ones_sum(&p[range]);
    p[at..at + 2].copy_from_slice(&(!sum).to_be_bytes());
}

/// RFC 1071 one's-complement sum of 16-bit words.
fn ones_sum(b: &[u8]) -> u16 {
    let mut sum: u32 = b.chunks(2).map(|c| u32::from(c[0]) << 8 | u32::from(*c.get(1).unwrap_or(&0))).sum();
    while sum > 0xffff {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    sum as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tunnel_core::Client;

    /// An ICMP echo request from 10.66.0.2 to the gateway, as the phone's kernel would write it.
    fn ping() -> Vec<u8> {
        let mut p = vec![0x45, 0, 0, 28, 0, 1, 0, 0, 64, 1, 0, 0, 10, 66, 0, 2, 10, 66, 0, 1, 8, 0, 0, 0, 0, 7, 0, 1];
        set_checksum(&mut p, 10, 0..20);
        set_checksum(&mut p, 22, 20..28);
        p
    }

    fn checksum_ok(p: &[u8]) -> bool {
        ones_sum(p) == 0xffff
    }

    #[test]
    fn reply_has_swapped_addresses_and_valid_checksums() {
        let r = echo_reply(&ping()).unwrap();
        assert_eq!(&r[12..20], &[10, 66, 0, 1, 10, 66, 0, 2]);
        assert_eq!(r[20], 0);
        assert!(checksum_ok(&r[..20]) && checksum_ok(&r[20..]));
    }

    #[test]
    fn only_pings_to_the_gateway_get_replies() {
        let mut p = ping();
        p[19] = 9;
        assert!(echo_reply(&p).is_none());
        assert!(echo_reply(&[0x45]).is_none());
    }

    #[test]
    fn short_header_length_does_not_panic() {
        // IHL 0: the length check alone passes, and reading byte 9 used to crash the relay.
        assert!(echo_reply(&[0x40, 0, 0, 0, 0, 0, 0, 0]).is_none());
        assert!(echo_reply(&[0x41; 12]).is_none());
    }

    /// Starts a relay with the given key bytes on a free port and returns its address.
    fn start_relay(key_bytes: &[u8]) -> std::net::SocketAddr {
        let keys = ServerKeys::from_bytes(key_bytes).unwrap();
        let server = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        std::thread::spawn(move || serve(&keys, &server));
        addr
    }

    fn client_socket() -> UdpSocket {
        let client = UdpSocket::bind("127.0.0.1:0").unwrap();
        client.set_read_timeout(Some(Duration::from_millis(500))).unwrap();
        client
    }

    /// The next packet from the relay as the client sees it: Some(None) for an ACK.
    fn receive(socket: &UdpSocket, client: &mut Client) -> Option<Option<Vec<u8>>> {
        let mut buf = [0u8; 2048];
        let n = socket.recv(&mut buf).ok()?;
        client.receive(&buf[..n]).ok()
    }

    /// Generates relay keys, starts a relay and completes a handshake with it.
    fn connected() -> (UdpSocket, std::net::SocketAddr, Client, Vec<u8>, Vec<u8>) {
        let keys = ServerKeys::generate();
        let addr = start_relay(&keys.to_bytes());
        let socket = client_socket();
        let (mut client, init) = tunnel_core::connect(keys.public()).unwrap();
        socket.send_to(&init, addr).unwrap();
        assert_eq!(receive(&socket, &mut client), Some(None), "expected the ACK");
        (socket, addr, client, init, keys.to_bytes())
    }

    #[test]
    fn ping_through_the_tunnel_end_to_end() {
        let (socket, addr, mut client, _, _) = connected();
        socket.send_to(&client.seal(&ping()).unwrap(), addr).unwrap();
        assert_eq!(receive(&socket, &mut client), Some(echo_reply(&ping())));
    }

    #[test]
    fn replaying_init_does_not_reopen_old_packets() {
        let (socket, addr, mut client, init, _) = connected();
        let recorded = client.seal(&ping()).unwrap();
        socket.send_to(&recorded, addr).unwrap();
        assert!(receive(&socket, &mut client).is_some());

        // An attacker resends the recorded INIT and DATA: the relay repeats the same ACK, which
        // the client ignores, and does not answer the old ping again.
        socket.send_to(&init, addr).unwrap();
        assert_eq!(receive(&socket, &mut client), Some(None));
        socket.send_to(&recorded, addr).unwrap();
        assert!(receive(&socket, &mut client).is_none());

        // The client's session still works.
        socket.send_to(&client.seal(&ping()).unwrap(), addr).unwrap();
        assert_eq!(receive(&socket, &mut client), Some(echo_reply(&ping())));
    }

    #[test]
    fn replies_follow_the_clients_latest_address() {
        let (_old, addr, mut client, _, _) = connected();
        let moved = client_socket(); // the phone switched from Wi-Fi to mobile data
        moved.send_to(&client.seal(&ping()).unwrap(), addr).unwrap();
        assert_eq!(receive(&moved, &mut client), Some(echo_reply(&ping())));
    }

    #[test]
    fn tunnel_recovers_after_a_relay_restart() {
        let (socket, addr, mut client, init, key_bytes) = connected();
        let recorded = client.seal(&ping()).unwrap();
        socket.send_to(&recorded, addr).unwrap();
        assert!(receive(&socket, &mut client).is_some());

        // A restarted relay: same long-term key, no memory of the session.
        let restarted = start_relay(&key_bytes);
        socket.send_to(&recorded, restarted).unwrap();
        assert!(receive(&socket, &mut client).is_none(), "old packets must not open on the new relay");
        socket.send_to(&init, restarted).unwrap(); // the client's periodic INIT resend
        assert_eq!(receive(&socket, &mut client), Some(None));
        socket.send_to(&client.seal(&ping()).unwrap(), restarted).unwrap();
        assert_eq!(receive(&socket, &mut client), Some(echo_reply(&ping())));
    }
}
