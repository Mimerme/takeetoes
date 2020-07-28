use crate::{PeerList, Peers, Pings};
use diffy::{apply, create_patch, Patch};
use enumn::N;
use log::{debug, error, info};
use sha2::{Digest, Sha256, Sha512};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Error, ErrorKind, Read, Result, Write};
use std::mem::size_of;
use std::net::{Incoming, IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::str::FromStr;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime};

//Change how many bytes we reserve in the protocol for the data length.
//The code bellow should use sizeof() when using this type
type DataLen = u8;
type OpCode = u8;
type Command = (OpCode, DataLen, Vec<u8>);

//Network Opcodes. support for 255
#[derive(N, Debug)]
#[repr(u8)]
pub enum NetOp {
    Intro = 1,
    IntroRes = 2,
    Ad = 3,
    Msg = 4,
    Ping = 5,
    Pong = 6,
    PROJ = 7,
    PROJ_VER = 8,
    PATCH = 9,
    VOTE_ASK = 10,
    VOTE_RES = 11,
}

pub fn recv_command(connection: &mut TcpStream, block: bool) -> Result<Command> {
    let mut network_header = vec![0; size_of::<OpCode>() + size_of::<DataLen>()];

    connection
        .set_nonblocking(!block)
        .expect("set_nonblocking failed");

    // Keep peeking until we have a network header in the connection stream

    //Block until an entire command is received
    if block {
        connection.read_exact(&mut network_header);

        //Read in the opcode and datalen bytes and convert them to usizes
        let mut opcode_bytes = [0; size_of::<usize>()];
        let mut index = 0;
        for x in (size_of::<usize>() - size_of::<OpCode>())..size_of::<usize>() {
            opcode_bytes[x] = network_header[index];
            index += 1;
        }
        let opcode = usize::from_be_bytes(opcode_bytes);
        let opcode: OpCode = opcode as OpCode;

        let mut data_len_bytes = [0; size_of::<usize>()];
        for x in (size_of::<usize>() - size_of::<DataLen>())..size_of::<usize>() {
            data_len_bytes[x] = network_header[index];
            index += 1;
        }
        let data_len = usize::from_be_bytes(data_len_bytes);

        //Begin waiting for the rest of the data for the command
        let mut command = vec![0; data_len];
        let data_len: DataLen = data_len as DataLen;

        //NOTE: weird edge case, probably can compact
        if data_len != 0 {
            connection.read_exact(&mut command);
            debug!(
                "Received command (blocking): {:?} | {:?} | {:?}",
                network_header[0], data_len, command
            );
            return Ok((opcode, data_len, command));
        } else {
            debug!(
                "Received command (blocking): {:?} | {:?}",
                network_header[0], data_len
            );
        }

        return Ok((opcode, 0, vec![]));
    } else {
        let ret = connection.peek(&mut network_header)?;

        if ret == 0 {
            return Err(Error::new(ErrorKind::UnexpectedEof, "EOF"));
        } else if ret < network_header.len() {
            return Err(Error::new(ErrorKind::WouldBlock, "would block"));
        }

        //Read in the opcode and datalen bytes and convert them to usizes
        let mut opcode_bytes = [0; size_of::<usize>()];
        let mut index = 0;
        for x in (size_of::<usize>() - size_of::<OpCode>())..size_of::<usize>() {
            opcode_bytes[x] = network_header[index];
            index += 1;
        }
        let opcode = usize::from_be_bytes(opcode_bytes);
        let opcode: OpCode = opcode as OpCode;

        let mut data_len_bytes = [0; size_of::<usize>()];
        for x in (size_of::<usize>() - size_of::<DataLen>())..size_of::<usize>() {
            data_len_bytes[x] = network_header[index];
            index += 1;
        }
        let data_len = usize::from_be_bytes(data_len_bytes);

        let mut command = vec![0; data_len + size_of::<DataLen>() + size_of::<OpCode>()];
        let data_len = data_len as DataLen;

        let ret = connection.peek(&mut command)?;
        if ret == 0 {
            return Err(Error::new(ErrorKind::UnexpectedEof, "EOF"));
        } else if ret < command.len() {
            return Err(Error::new(ErrorKind::WouldBlock, "would block"));
        }

        connection.read_exact(&mut command);

        let data = &command[size_of::<DataLen>() + size_of::<OpCode>()..];

        debug!(
            "Received command (non-blocking): {:?} | {:?} | {:?}",
            opcode, data_len, data
        );
        return Ok((opcode, data_len, data.to_vec()));
    }
}

//Wrapper for send_command()
pub fn send(command: Command, connection: &mut TcpStream) -> Result<()> {
    return send_command(
        NetOp::n(command.0).unwrap(),
        command.1,
        &command.2,
        connection,
    );
}

//Send a command to a TCPStream
pub fn send_command(
    opcode: NetOp,
    data_len: DataLen,
    data: &Vec<u8>,
    connection: &mut TcpStream,
) -> Result<()> {
    let mut data = data.clone();
    debug!(
        "Sending command: {:?} | {:?} | {:?}",
        opcode, data_len, data
    );
    //Add the headers to the outgoing data by concating 3 vectors together
    let mut op_be = (opcode as OpCode).to_be_bytes().to_vec();
    let mut dl_be = data_len.to_be_bytes().to_vec();
    dl_be.extend(data.iter());
    op_be.extend(dl_be.iter());
    connection.write_all(&op_be);

    return Ok(());
}

pub fn connect(
    connecting_ip: &str,
    binding_ip: &str,
    mut peers: Peers,
    mut peer_list: PeerList,
    mut pings: Pings,
) -> Result<()> {
    debug!("Connecting to {}...", connecting_ip);

    //Connect to the initial client stream
    let mut stream = TcpStream::connect(connecting_ip.clone())?;
    let mut stream_clone = stream.try_clone().unwrap();
    let host_stream_addr = stream.peer_addr().unwrap();

    //Creating the 'advertised ip' by converting the binding_ip into bytes
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
    send_command(NetOp::Intro, 6, &ip, &mut stream);
    //Intro peer responds with peer list
    println!("waiting");
    let (mut res_op, mut res_len, mut res_data) = recv_command(&mut stream, true).unwrap();

    let mut peer_data = res_data.clone();
    let host_addr = stream.peer_addr().unwrap();
    debug!("Intro Response: {:?}", (res_op, res_len, res_data));

    //Peer List network format is in:
    //ip1 (4 bytes) | port1 (2 bytes) | ip2 ...
    peers
        .write()
        .unwrap()
        .insert(host_addr, (Mutex::new(stream_clone), Mutex::new(stream)));
    peer_list
        .write()
        .unwrap()
        .insert(host_addr, SocketAddr::from_str(&connecting_ip).unwrap());

    //Add this initial connection to the peer list
    pings
        .write()
        .unwrap()
        .insert(host_stream_addr, (1, SystemTime::now()));

    debug!("Starting peer connections...");
    // Connect to each of the specified peers
    for start in (0..res_len.into()).step_by(6) {
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

        debug!("Connecting to ip={} port={:?}", peer_addr, port_number);
        //Do not allow self referencing peers
        if binding_sock == peer_addr {
            error!("Found self referencing peers");
            continue;
        }

        //Connect to the specified peer
        let mut stream = TcpStream::connect(peer_addr).unwrap();
        let host_addr = stream.peer_addr().unwrap();

        //Send an AD command to add node to the network's peer list
        send_command(NetOp::Ad, 6, &ip, &mut stream);

        //TODO: work on removing this
        //if verify_file_hash(proj_hash, file_hash, &mut stream) == false {
        //panic!("Different filehash than network");
        //}

        //Update the pings and add to the peers
        pings
            .write()
            .unwrap()
            .insert(stream.peer_addr().unwrap(), (1, SystemTime::now()));

        peers.write().unwrap().insert(
            host_addr,
            (Mutex::new(stream.try_clone().unwrap()), Mutex::new(stream)),
        );

        peer_list.write().unwrap().insert(host_addr, peer_addr);
        println!("Successfully connected to Peer: {:?}", peer_addr);
    }

    debug!("Connected to all peers!");
    return Ok(());
}

pub fn accept_connections(mut ping_status: Pings, mut peers: Peers, mut listener: TcpListener) {
    //Accept every incomming TCP connection on the main thread
    for stream in listener.incoming() {
        debug!("New connection");
        //Stream ownership is passed to the thread
        let mut tcp_connection: TcpStream = stream.unwrap();
        let peer_addr = tcp_connection.peer_addr().unwrap();

        //Add to the peer list
        ping_status
            .write()
            .unwrap()
            .insert(tcp_connection.peer_addr().unwrap(), (1, SystemTime::now()));

        //NOTE: peers is set to None here. potential unsafety
        peers.write().unwrap().insert(
            tcp_connection.peer_addr().unwrap(),
            (
                Mutex::new(tcp_connection.try_clone().unwrap()),
                Mutex::new(tcp_connection),
            ),
        );
    }
}
