use crate::threads::RunOp;
use crate::{PeerList, Peers, Pings};
use enumn::N;
use log::{debug, error, info};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Error, ErrorKind, Read, Result, Write};
use std::mem::size_of;
use std::net::{Incoming, IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::str::FromStr;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime};
use stoppable_thread::SimpleAtomicBool;

//Change how many bytes we reserve in the protocol for the data length.
//The code bellow should use sizeof() when using this type
type DataLen = u8;
type OpCode = u8;

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
    Broadcast = 42,
    PROJ = 7,
    PROJ_VER = 8,
    PATCH = 9,
    VOTE_ASK = 10,
    VOTE_RES = 11,
}

pub fn recv_command(connection: &mut TcpStream, block: bool) -> Result<(OpCode, DataLen, Vec<u8>)> {
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
/*pub fn send(command: Command, connection: &mut TcpStream) -> Result<()> {
    return send_command(
        NetOp::n(command.0).unwrap(),
        command.1,
        &command.2,
        connection,
    );
}*/

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
