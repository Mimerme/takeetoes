use diffy::{apply, create_patch, Patch};
use log::{debug, info};
use sha2::{Digest, Sha256, Sha512};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Error, ErrorKind};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime};

//Network Opcodes. support for 255
#[repr(u8)]
enum NetOp {
    INTRO,
    INTRO_RES,
    AD,
    MSG,
    PING,
    PONG,
    PROJ,
    PROJ_VER,
    PATCH,
    VOTE_ASK,
    VOTE_RES,
}

fn recv_command(connection: &mut TcpStream, block: bool) -> Result<(u8, u64, Vec<u8>), ()> {
    let net_debug = true;
    let mut network_header = vec![0; 9];

    connection
        .set_nonblocking(!block)
        .expect("set_nonblocking failed");
    // Keep peeking until we have a network header in the connection stream
    //println!("recv_command()");

    //println!("Loop: {:?}", block);
    //Block until an entire command is received
    if block {
        //println!("Blocking...");
        connection.read_exact(&mut network_header);
        let data_len = u64::from_be_bytes(&network_header[1..]);
        let dl_index: usize = data_len.into();
        let mut command = vec![0; dl_index];

        //println!("Waiting for data... {:?}", dl_index);
        if data_len != 0 {
            connection.read_exact(&mut command);
            if net_debug {
                println!(
                    "Received command (blocking): {:?} | {:?} | {:?}",
                    network_header[0], data_len, command
                );
            }
            return Ok((network_header[0], data_len, command));
        } else {
            if net_debug {
                println!(
                    "Received command (blocking): {:?} | {:?}",
                    network_header[0], data_len
                );
            }
            return Ok((network_header[0], 0, vec![]));
        }
    } else {
        let ret = connection.peek(&mut network_header)?;

        if ret == 0 {
            return Err(Error::new(ErrorKind::UnexpectedEof, "EOF"));
        } else if ret < network_header.len() {
            return Err(Error::new(ErrorKind::WouldBlock, "would block"));
        }
        let data_len = u64::from_be_bytes(&network_header[1..]);
        let dl_index: usize = data_len.into();

        let mut command = vec![0; dl_index + 1 + 8];
        //println!("NO BLOCK2 {}", command.len());
        let ret = connection.peek(&mut command)?;
        if ret == 0 {
            return Err(Error::new(ErrorKind::UnexpectedEof, "EOF"));
        } else if ret < command.len() {
            //println!("YYEEET");
            return Err(Error::new(ErrorKind::WouldBlock, "would block"));
        }

        connection.read_exact(&mut command);

        if net_debug {
            let opcode = command[0];
            let data = &command[8..(dl_index + 8)];

            println!(
                "Received command (non-blocking): {:?} | {:?} | {:?}",
                opcode, data_len, data
            );
        }
        return Ok((command[0], data_len, command[8..(dl_index + 8)].to_vec()));
    }
}

//Send a command to a TCPStream
fn send_command(
    opcode: u8,
    data_len: u64,
    data: &Vec<u8>,
    connection: &mut TcpStream,
) -> Result<()> {
    let net_debug = true;
    let mut data = data.clone();
    if net_debug {
        println!(
            "Sending command: {:?} | {:?} | {:?}",
            opcode, data_len, data
        );
    }
    //Add the headers to the outgoing data
    let be_bytes = data_len.to_be_bytes();
    data.insert(0, be_bytes[0]);
    data.insert(0, be_bytes[1]);
    data.insert(0, be_bytes[2]);
    data.insert(0, be_bytes[3]);
    data.insert(0, be_bytes[4]);
    data.insert(0, be_bytes[5]);
    data.insert(0, be_bytes[6]);
    data.insert(0, be_bytes[7]);
    data.insert(0, opcode);
    connection.write_all(&data);

    return Ok(());
}

//Sends the syncronization packets to the peer network
fn send_sync(
    mut connection: &mut TcpStream,
    old_contents: &str,
    new_contents: &str,
    file_bufs: &mut HashMap<String, (String, String)>,
    net_path: &str,
    version_history: &mut HashMap<[u8; 64], (String, [u8; 64])>,
) {
    let patch = create_patch(old_contents, new_contents);

    //create_patch() doesn't allow us to set the 'original' and 'modified' headers so do it after its
    //created
    let mut patch = patch.to_string();
    patch = patch.replacen("original", net_path);
    patch = patch.replacen("modified", net_path);

    //Generate a hash id that corresponds to the file version that the patch should be applied to
    let mut old_hasher = Sha512::new();
    old_hasher.update(old_contents.as_bytes());
    old_hasher.update(net_path.as_bytes());
    let old_hash = old_hasher.finalize();
    old_hash.extend(patch.as_bytes());

    let mut new_hasher = Sha512::new();
    new_hasher.update(new_contents.as_bytes());
    new_hasher.update(net_path.as_bytes());

    println!("Patch\n========\n{:?}", patch);

    //Send the difference between two file versions
    send_command(
        NetOp::PATCH,
        old_hash.len() as u64,
        old_hash,
        &mut connection,
    );

    //Update the project file buffer through the mutable reference
    file_bufs.get_mut(net_path.to_str()).unwrap().1 = new_contents.clone();
    //Update the version history
    //(update the commit related to the old contents [the link]
    //and insert the comit related to the new content)
    version_history.insert(&new_hasher.finalize(), (new_contents.clone(), None));
    version_history.get_mut(&old_hash.finalize()).1 = Some(&new_hasher.finalize());
}

pub fn connect(connecting_ip: &str, binding_ip: &str, peer_ads) {
    debug!("Connecting to {}...", connecting_ip);

    //Connect to the initial client stream
    let mut stream = TcpStream::connect(connecting_ip.clone())?;
    let mut stream_clone = stream.try_clone().unwrap();

    //Creating the 'advertised ip' by converting the binding_ip[ into bytes
    let binding_sock = SocketAddr::from_str(&binding_ip).unwrap();
    let mut ip = match binding_sock.ip() {
        IpAddr::V4(ip) => ip.octets().to_vec(),
        IpAddr::V6(ip) => panic!("Protocol currently doesn't support ipv6 :("),
    };
    let port: u16 = binding_sock.port();
    ip.push((port >> 8) as u8);
    ip.push(port as u8);

    //Begin asking for the network's peer list
    //INTRO packet and an advertised peer list
    //[opcode (1), data_length (8), ip : port (6)]
    debug!("Introducing IP {:?}", binding_sock);
    send_command(NetOp::INTRO, 6, &ip, &mut stream);
    //Intro peer responds with peer list
    let (mut res_op, mut res_len, mut res_data) = recv_command(&mut stream, true).unwrap();

    let mut peer_data = res_data.clone();
    debug!("Intro Response: {:?}", (res_op, res_len, res_data));

    //Peer List network format is in:
    //ip1 (4 bytes) | port1 (2 bytes) | ip2 ...
    peer_ads.write().unwrap().insert(
        stream.peer_addr().unwrap(),
        SocketAddr::from_str(&connecting_ip).unwrap(),
    );

    //Add this initial connection to the peer list
    ping_status
        .write()
        .unwrap()
        .insert(stream.peer_addr().unwrap(), (1, SystemTime::now()));
    peers.write().unwrap().insert(
        stream.peer_addr().unwrap(),
        (Mutex::new(stream_clone), Mutex::new(stream)),
    );

    debug!("Starting peer connections...");
    // Connect to each of the specified peers
    for start in (0..res_len.into()).step_by(6) {
        println!("ONE");
        //https://stackoverflow.com/questions/50243866/how-do-i-convert-two-u8-primitives-into-a-u16-primitive?noredirect=1&lq=1
        let port_number: u16 = ((peer_data[start + 4] as u16) << 8) | peer_data[start + 5] as u16;
        let peer_addr = SocketAddr::from((
            [
                peer_data[start],
                peer_data[start + 1],
                peer_data[start + 2],
                peer_data[start + 3],
            ],
            port_number,
        ));

        //Do not allow self referencing peers
        if binding_sock == peer_addr {
            println!("Found self referencing peers");
            continue;
        }

        //Connect to the specified peer
        println!("Peer: {:?}", peer_addr);
        let mut stream = TcpStream::connect(peer_addr).unwrap();

        //Send an AD command to add node to the network's peer list
        send_command(AD, 6, &ip, &mut stream);

        if verify_file_hash(proj_hash, file_hash, &mut stream) == false {
            panic!("Different filehash than network");
        }

        ping_status
            .write()
            .unwrap()
            .insert(stream.peer_addr().unwrap(), (1, SystemTime::now()));
        peer_ads
            .write()
            .unwrap()
            .insert(stream.peer_addr().unwrap(), peer_addr);
        peers.write().unwrap().insert(
            stream.peer_addr().unwrap(),
            (Mutex::new(stream.try_clone().unwrap()), Mutex::new(stream)),
        );
    }

    println!("Connected to all peers");
}
