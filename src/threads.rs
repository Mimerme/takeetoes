//functions to spawn all the threads needed by the node
//NOTE: if you're looking for the debug thread it's in the main function
//
use crate::tak_net::{recv_command, send_command, DataLen, NetOp, OpCode};
use crate::{Node, PeerList, Peers, Pings};
use enumn::N;
use log::{debug, error, info};
use std::cmp::PartialEq;
use std::collections::{HashMap, HashSet, VecDeque};
use std::convert::TryInto;
use std::io::Write;
use std::io::{Error, ErrorKind};
use std::net::{Incoming, IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::rc::Rc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};
use std::{thread, time};
use stoppable_thread::StoppableHandle;

//How many seconds no activity may go on with another peer before a PING is sent
const KEEP_ALIVE: usize = 60;
//How many milliseconds the network thread should be delayed for
const NET_DELAY: usize = 250;
//How many milliseconds the accept thread should wait after each block
const ACCEPT_DELAY: usize = 250;
//How many milliseconds the two IPC threads should wait after each block
const IPC_DELAY: usize = 250;

//Enum used to communicate between channels in the runtime
#[derive(Debug, PartialEq)]
#[repr(u8)]
pub enum RunOp {
    PingReq,
    PingRes(Vec<(SocketAddr, u8, u64)>),
    Broadcast(SocketAddr, Vec<u8>),
    OnJoin(SocketAddr),
    OnLeave(SocketAddr),
}

//Enum used to communicate with other processes
#[derive(N, Debug)]
#[repr(u8)]
pub enum IpcOp {
    PingReq = 1,
    PingRes = 2,
    Broadcast = 3,
    OnJoin = 4,
    OnLeave = 5,
}

pub type Command = RunOp;
pub type NodeHandles = (
    StoppableHandle<()>,
    StoppableHandle<()>,
    StoppableHandle<()>,
);

pub fn sock_addr_to_bytes(addr: SocketAddr) -> Vec<u8> {
    let mut ip = match addr.ip() {
        IpAddr::V4(ip) => ip.octets().to_vec(),
        IpAddr::V6(ip) => panic!("Protocol currently doesn't support ipv6 :("),
    };
    let port: u16 = addr.port();
    ip.push((port >> 8) as u8);
    ip.push(port as u8);

    return ip;
}

pub fn start_network_thread(
    mut peers: Peers,
    mut peer_list: PeerList,
    mut ping_status: Pings,
    mut ret_nodein_recv: Receiver<Command>,
    mut ipc_nodein_recv: Receiver<Command>,
    mut ret_nodeout_send: Sender<Command>,
    mut ipc_nodeout_send: Sender<Command>,
    rdy_flag: Arc<AtomicU8>,
) -> StoppableHandle<()> {
    //Thread to run the main event loop / protocol
    //This thread only deals with peers who have been learned / connected with
    return stoppable_thread::spawn(move |stopped| {
        let mut peers_to_remove = Vec::new();

        debug!("=====MAIN EVENT LOOP STARTED=====");

        //TODO: this might not actually set the thread state as rdy on rlly slow machines???
        //Set the thread state as rdy for the node thread from which this was called
        let flag = rdy_flag.load(Ordering::Relaxed) | 0b00000100;
        rdy_flag.store(flag, Ordering::Relaxed);

        while !stopped.get() && rdy_flag.load(Ordering::Relaxed) != 0b11111111 {
            let mut peer_index = 0;

            //Remove the peers before we obtain the lock and result in a deadlock
            if !peers_to_remove.is_empty() {
                for peer in peers_to_remove.drain(..) {
                    peers.write().unwrap().remove(&peer);

                    ret_nodeout_send.send(RunOp::OnLeave(peer.clone()));
                    ipc_nodeout_send.send(RunOp::OnLeave(peer.clone()));
                }
            }

            for (_, (read, write)) in peers.read().unwrap().iter() {
                //Get the TcpStream used explicitly for reading from the socket
                let mut recv = read.lock().unwrap();
                match recv_command(&mut recv, false) {
                    Ok((opcode, len, mut data)) => {
                        //Update the socket's entry in the ping table whenever we receive a new command
                        ping_status
                            .write()
                            .unwrap()
                            .insert(recv.peer_addr().unwrap(), (1, SystemTime::now()));

                        //Check for the opcode of the data first
                        match NetOp::n(opcode) {
                            Some(NetOp::Intro) => {
                                debug!("Responding to an INTRO...");

                                //Get a 'read only' copy of the peer list and the lock to the write
                                //stream
                                let wr = write;
                                let mut write = wr.lock().unwrap();

                                let ad_ip_data = data;
                                //TODO: support ipv6
                                //Serialize the peer_list into bytes
                                let mut data = vec![];
                                for (_, peer) in peer_list.read().unwrap().iter() {
                                    let mut ip = match peer.ip() {
                                        IpAddr::V4(ip) => ip.octets().to_vec(),
                                        IpAddr::V6(ip) => {
                                            panic!("Protocol currently doesn't support ipv6 :(")
                                        }
                                    };
                                    let port: u16 = peer.port();
                                    ip.push((port >> 8) as u8);
                                    ip.push(port as u8);
                                    data.extend(ip.iter());
                                }

                                send_command(
                                    NetOp::IntroRes as u8,
                                    (peer_list.read().unwrap().len() * 6) as u8,
                                    &data,
                                    &mut write,
                                );
                                debug!("Responded with peer list");

                                let port_number: u16 =
                                    ((ad_ip_data[4] as u16) << 8) | ad_ip_data[5] as u16;
                                let advertised_addr = SocketAddr::from((
                                    [ad_ip_data[0], ad_ip_data[1], ad_ip_data[2], ad_ip_data[3]],
                                    port_number,
                                ));

                                peer_list
                                    .write()
                                    .unwrap()
                                    .insert(write.peer_addr().unwrap(), advertised_addr);

                                //Nodes aren't 'joined' until they either send an
                                //INTRO or AD
                                ret_nodeout_send.send(RunOp::OnJoin(advertised_addr));
                                ipc_nodeout_send.send(RunOp::OnJoin(advertised_addr));

                                debug!(
                                    "Added {:?}'s advertisement address to the peer_list (INTRO)",
                                    advertised_addr
                                );
                            }
                            Some(NetOp::Ad) => {
                                debug!("SAVING AD");
                                let ad_ip_data = data;
                                let port_number: u16 =
                                    ((ad_ip_data[4] as u16) << 8) | ad_ip_data[5] as u16;
                                let advertised_addr = SocketAddr::from((
                                    [ad_ip_data[0], ad_ip_data[1], ad_ip_data[2], ad_ip_data[3]],
                                    port_number,
                                ));

                                //Update the peer and advertisement list with the addr
                                peer_list.write().unwrap().insert(
                                    write.lock().unwrap().peer_addr().unwrap(),
                                    advertised_addr,
                                );

                                //Nodes aren't 'joined' until they either send an
                                //INTRO or AD
                                ret_nodeout_send.send(RunOp::OnJoin(advertised_addr));
                                ipc_nodeout_send.send(RunOp::OnJoin(advertised_addr));

                                debug!("Added {:?} to the peer_list (AD)", advertised_addr);
                            }
                            Some(NetOp::Msg) => {
                                println!("MSG: {}", String::from_utf8(data.to_vec()).unwrap());
                            }
                            Some(NetOp::Ping) => {
                                debug!("Received a PING");
                                send_command(
                                    NetOp::Pong as u8,
                                    0,
                                    &vec![],
                                    &mut write.lock().unwrap(),
                                );
                                debug!("Sent a PONG");
                            }
                            Some(NetOp::Pong) => {
                                debug!("Receivied a PONG");
                                //Update the ping table
                                ping_status
                                    .write()
                                    .unwrap()
                                    .insert(recv.peer_addr().unwrap(), (1, SystemTime::now()));
                            }
                            //All IPC and other non Takeetoe related network is moved as datagram
                            //packets.
                            //If we receive a Broadcast packet then clone and send that information
                            //along the channels
                            //This is the __TakeePC Protocol__
                            Some(NetOp::Broadcast) => {
                                let peer_addr = recv.peer_addr().unwrap();
                                let peer_list = peer_list.read().unwrap();
                                let peer_addr = peer_list.get(&peer_addr).unwrap();

                                debug!("Network BROADCAST received!");
                                ret_nodeout_send.send(RunOp::Broadcast(*peer_addr, data.clone()));
                                ipc_nodeout_send.send(RunOp::Broadcast(*peer_addr, data));
                            }
                            _ => {
                                println!("??? Unknown OpCode ???: ({:?}, Length: {:?})", data, len);
                            }
                        }
                    }
                    Err(ref e) if e.kind() == ErrorKind::WouldBlock => {}
                    Err(ref e) if e.kind() == ErrorKind::UnexpectedEof => {
                        debug!("Peer has disconnected!");
                        let host_sock = recv.peer_addr().unwrap();

                        peer_list.write().unwrap().remove(&host_sock);

                        //NOTE: the bellow Results in a deadlock
                        //peers.write().unwrap().remove(&host_sock);
                        ping_status.write().unwrap().remove(&host_sock);

                        peers_to_remove.push(host_sock);
                    }
                    Err(e) => {
                        panic!(e);
                    }
                }
                peer_index += 1;
            }

            /*
             *
             * Ping table handling begins bellow...
             *
             *
             * */
            let now = SystemTime::now();
            let read_only = ping_status.read().unwrap().clone();

            //Update the ping table if needed
            for (host_sock, (status, last_command)) in read_only.iter() {
                let seconds_elapsed = last_command.elapsed().unwrap().as_secs();

                //If more than 10 seconds have elapsed since the last update then send
                //a ping
                if seconds_elapsed >= KEEP_ALIVE as u64 && *status == 1u8 {
                    let mut write = peers.write().unwrap();
                    let mut write = &write.get_mut(host_sock).unwrap().1;

                    debug!(
                        "Sending a PING to {:?} (host socket) {:?}",
                        host_sock, write,
                    );
                    send_command(NetOp::Ping as u8, 0, &vec![], &mut write.lock().unwrap());
                    ping_status
                        .write()
                        .unwrap()
                        .insert(*host_sock, (2, SystemTime::now()));
                }
                //If more than 10 seconds have elapsed since the ping then assume the
                //peer is dead
                else if seconds_elapsed >= 60 && *status == 2u8 {
                    debug!("NO REPSONSE FROM PEER. REMOVED");
                    //Since we're iterating over a non-clonable iterator schedule
                    //sockets to remove when we return to the top of the loop
                    //ping_status_clone.write().unwrap().remove(host_sock);
                    peers.write().unwrap().remove(host_sock);
                    ping_status.write().unwrap().remove(host_sock);
                    //ping_status.write().unwrap().insert(*host_sock, (0, SystemTime::now()));
                    //println!("DONE!");
                }
            }

            /*
             * Handle all TakeePC Protocol commands from the IPC thread and native thread
             *
             * */

            //Start by handling the ipc thread
            match ipc_nodein_recv.try_recv() {
                //Handle the special runtime ping request
                Ok(RunOp::PingReq) => {
                    let mut data: Vec<(SocketAddr, u8, u64)> = Vec::new();

                    //Each Entry is 6 + 1 + 8 bytes = 15 bytes
                    //net_addr | status | seconds since epoch
                    for (host_addr, (status, time)) in ping_status.read().unwrap().iter() {
                        let peer_list = peer_list.read().unwrap();
                        let net_addr = peer_list.get(host_addr).unwrap();

                        let time_since_epoch = time
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .unwrap()
                            .as_secs() as u64;

                        data.push((net_addr.clone(), *status, time_since_epoch));
                    }
                    ipc_nodeout_send.send(RunOp::PingRes(data));
                }
                Ok(RunOp::Broadcast(peer, data)) => {
                    for (_, (read, write)) in peers.read().unwrap().iter() {
                        send_command(
                            NetOp::Broadcast as u8,
                            data.len().try_into().unwrap(),
                            &data,
                            &mut write.lock().unwrap(),
                        );
                    }
                }
                Ok(_) => panic!("Shouldn't be receiving a anything else"),
                Err(e) => { /*continue if there is nothing to do*/ }
            }

            //Then handle the native thread
            match ret_nodein_recv.try_recv() {
                //Handle the special runtime ping request
                Ok(RunOp::PingReq) => {
                    let mut data: Vec<(SocketAddr, u8, u64)> = Vec::new();

                    //Each Entry is 6 + 1 + 8 bytes = 15 bytes
                    //net_addr | status | seconds since epoch
                    for (host_addr, (status, time)) in ping_status.read().unwrap().iter() {
                        let peer_list = peer_list.read().unwrap();
                        let net_addr = peer_list.get(host_addr).unwrap();

                        let time_since_epoch = time
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .unwrap()
                            .as_secs() as u64;

                        data.push((net_addr.clone(), *status, time_since_epoch));
                    }
                    ret_nodeout_send.send(RunOp::PingRes(data));
                }
                Ok(RunOp::Broadcast(peer, data)) => {
                    for (_, (read, write)) in peers.read().unwrap().iter() {
                        send_command(
                            NetOp::Broadcast as u8,
                            data.len().try_into().unwrap(),
                            &data,
                            &mut write.lock().unwrap(),
                        );
                    }
                }
                Ok(_) => panic!("Shouldn't be receiving a anything else"),
                Err(_) => {}
            }

            thread::sleep(Duration::from_millis(NET_DELAY as u64));
        }

        //println!("Shutting down");
        ////Begin a graceful shutdown
        //for (_, (_, write)) in peers.read().unwrap().iter() {
        //    write.lock().unwrap().shutdown(Shutdown::Both);
        //}

        //println!("Net thread shut down");
    });
}

pub fn start_ipc_thread(
    port: String,
    mut peers: Peers,
    mut ping_status: Pings,
    mut peer_list: PeerList,
    mut ipc_nodein_send: Sender<Command>,
    mut ipc_nodeout_recv: Receiver<Command>,
    rdy_flag: Arc<AtomicU8>,
) -> StoppableHandle<()> {
    return stoppable_thread::spawn(move |stopped| {
        let ipc_listener = TcpListener::bind(format!("127.0.0.1:{}", port)).unwrap();

        ipc_listener.set_nonblocking(true);

        //TODO: this might not actually set the thread state as rdy on rlly slow machines???
        //Set the thread state as rdy for the node thread from which this was called
        let flag = rdy_flag.load(Ordering::Relaxed) | 0b00000010;
        rdy_flag.store(flag, Ordering::Relaxed);
        //Handle the first incomming connection
        //setup this closure so that we can stop the thread from the outside

        let rdy_flag_clone = rdy_flag.clone();
        let stopptable_accept = move || -> Option<TcpStream> {
            while !stopped.get() && rdy_flag_clone.load(Ordering::Relaxed) != 0b11111111 {
                match ipc_listener.accept() {
                    Ok((stream, addr)) => {
                        return Some(stream);
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => continue,
                    Err(e) => panic!(e),
                }
            }
            return None;
        };
        let mut stream = match stopptable_accept() {
            Some(stream) => stream,
            None => return,
        };

        while !stopped.get() && rdy_flag.load(Ordering::Relaxed) != 0b11111111 {
            //stream.write(b"obama");

            //stream.set_nonblocking(true);

            //Forward all of nodeOut to the each net stream
            loop {
                let mut buf = Vec::new();
                match ipc_nodeout_recv.try_recv() {
                    Ok(command) => {
                        //Serialize the runOp and return the Result of send_command and
                        //see if the IPCconnection still remains open
                        let res = match command {
                            //Everything in bound should be of proper size so no need to
                            //resize
                            //Begin all the serialization garbage
                            RunOp::Broadcast(peer, msg) => {
                                let mut ip = match peer.ip() {
                                    IpAddr::V4(ip) => ip.octets().to_vec(),
                                    IpAddr::V6(ip) => {
                                        panic!("Protocol currently doesn't support ipv6 :(")
                                    }
                                };
                                let port: u16 = peer.port();
                                ip.push((port >> 8) as u8);
                                ip.push(port as u8);
                                buf.extend(ip.iter());
                                buf.extend(msg);

                                send_command(
                                    IpcOp::Broadcast as OpCode,
                                    buf.len() as DataLen,
                                    &buf,
                                    &mut stream,
                                )
                            }
                            RunOp::OnJoin(addr) => {
                                buf.extend(sock_addr_to_bytes(addr));
                                send_command(
                                    IpcOp::OnJoin as OpCode,
                                    buf.len() as DataLen,
                                    &buf,
                                    &mut stream,
                                )
                            }
                            RunOp::OnLeave(addr) => {
                                buf.extend(sock_addr_to_bytes(addr));
                                send_command(
                                    IpcOp::OnLeave as OpCode,
                                    buf.len() as DataLen,
                                    &buf,
                                    &mut stream,
                                )
                            }

                            RunOp::PingRes(res) => {
                                for (peer, ping_status, epoch_time) in res.iter() {
                                    buf.append(&mut sock_addr_to_bytes(*peer));
                                    buf.push(*ping_status);
                                    buf.append(&mut epoch_time.to_be_bytes().to_vec());
                                }

                                send_command(
                                    IpcOp::PingRes as OpCode,
                                    buf.len() as DataLen,
                                    &buf,
                                    &mut stream,
                                )
                            }
                            x => Err(std::io::Error::new(
                                ErrorKind::UnexpectedEof,
                                "Received invalid RunOp on nodeOut",
                            )),
                        };

                        match res {
                            Ok(()) => continue,
                            //If the connection is closed then schedule the client for
                            //removal
                            Err(ref e) if e.kind() == ErrorKind::WriteZero => {
                                println!("Ipc disconnected via write. Stopping node");
                                rdy_flag.store(0b11111111, Ordering::Relaxed);
                                return;
                            }
                            Err(e) => panic!(e),
                        }
                    }
                    //On an empty channel break out of the loop and process nodeIn
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => break,
                    Err(e) => panic!(e),
                }
            }

            //After processing client output then handle incoming node IO
            loop {
                //Forward all of the incoming data on the stream to nodeIn
                match recv_command(&mut stream, false) {
                    Ok((opcode, datalen, data)) => {
                        recv_command(&mut stream, false);
                        //IPCs can only used 2 opcodes
                        match IpcOp::n(opcode) {
                            Some(IpcOp::PingReq) => {
                                ipc_nodein_send.send(RunOp::PingReq);
                            }
                            Some(IpcOp::Broadcast) => {
                                ipc_nodein_send.send(RunOp::Broadcast(
                                    be_bytes_to_ip(&data[0..6]),
                                    data[6..datalen as usize].to_vec(),
                                ));
                            }
                            _ => panic!("Received {:?} on \'IPC in\'"),
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(ref e) if e.kind() == ErrorKind::UnexpectedEof => {
                        println!("Ipc disconnected via read. Stopping node");
                        rdy_flag.store(0b11111111, Ordering::Relaxed);
                        return;
                    }
                    Err(e) => panic!(e),
                }
            }
            thread::sleep(Duration::from_millis(IPC_DELAY as u64));
        }
    });
}

pub fn be_bytes_to_ip(data: &[u8]) -> SocketAddr {
    //TODO: support ipv6
    let peer_data = &data[0..6];
    let port_number: u16 = ((peer_data[4] as u16) << 8) | peer_data[5] as u16;

    let peer_addr = SocketAddr::from((
        [peer_data[0], peer_data[1], peer_data[2], peer_data[3]],
        port_number,
    ));

    return peer_addr;
}

pub fn start_accept_thread(
    mut peers: Peers,
    mut ping_status: Pings,
    mut listener: TcpListener,
    rdy_flag: Arc<AtomicU8>,
) -> StoppableHandle<()> {
    return stoppable_thread::spawn(move |stopped| {
        listener.set_nonblocking(true);

        //TODO: this might not actually set the thread state as rdy on rlly slow machines???
        //Set the thread state as rdy for the node thread from which this was called
        let flag = rdy_flag.load(Ordering::Relaxed) | 0b00000001;
        rdy_flag.store(flag, Ordering::Relaxed);

        //Accept every incomming TCP connection on the main thread
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    debug!("New connection");
                    //Stream ownership is passed to the thread
                    let mut tcp_connection: TcpStream = stream;
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
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if stopped.get() || rdy_flag.load(Ordering::Relaxed) == 0b11111111 {
                        return;
                    }
                    thread::sleep(Duration::from_millis(ACCEPT_DELAY.try_into().unwrap()));
                    continue;
                }
                Err(e) => {
                    panic!("IO error: {}", e);
                }
            }
        }
    });
}
