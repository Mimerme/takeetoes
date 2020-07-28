//functions to spawn all the threads needed by the node
//NOTE: if you're looking for the debug thread it's in the main function
use crate::tak_net::{accept_connections, recv_command, send_command, NetOp};
use crate::{PeerList, Peers, Pings};
use enumn::N;
use log::{debug, error, info};
use notify::event::{EventKind, MetadataKind, ModifyKind};
use notify::Config;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use sha2::{Digest, Sha256, Sha512};
use std::cmp::PartialEq;
use std::collections::{HashMap, HashSet, VecDeque};
use std::convert::TryInto;
use std::io::{Error, ErrorKind};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};
use std::{thread, time};
//How many seconds no activity may go on with another peer before a PING is sent
const KEEP_ALIVE: usize = 60;
//How many seconds the network thread should be delayed for
const NET_DELAY: usize = 0;

//Enum used to comm
#[derive(Debug, PartialEq)]
pub enum RunOp {
    PingReq,
    PingRes(Vec<(SocketAddr, u8, u64)>),
    Broadcast(Vec<u8>),
    OnJoin(SocketAddr),
}

//TODO: old shit. figure out what to remove
type DataLen = u8;
pub type Command = RunOp;

fn sock_addr_to_bytes(addr: SocketAddr) -> Vec<u8> {
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
) -> JoinHandle<()> {
    //Thread to run the main event loop / protocol
    //This thread only deals with peers who have been learned / connected with
    return thread::spawn(move || {
        //Peers to remove on the next iteration of the loop
        let mut peers_to_remove: HashSet<SocketAddr> = HashSet::new();

        debug!("=====MAIN EVENT LOOP STARTED=====");

        loop {
            //Remove all the pears from the event loop
            if peers_to_remove.len() > 0 {
                for peer in peers_to_remove.iter() {
                    peers.write().unwrap().remove(&peer);
                }
                peers_to_remove.drain();
            }

            let mut peer_index = 0;

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
                                    NetOp::IntroRes,
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
                                (*peer_list
                                    .write()
                                    .unwrap()
                                    .get_mut(&write.lock().unwrap().peer_addr().unwrap())
                                    .unwrap()) = advertised_addr;

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
                                send_command(NetOp::Pong, 0, &vec![], &mut write.lock().unwrap());
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
                                //TODO: support IPV6
                                //as an intermidary step modify the data len and data to include
                                //the net_addr that it came from
                                let peer_addr = recv.peer_addr().unwrap();
                                let peer_list = peer_list.read().unwrap();
                                let peer_addr = peer_list.get(&peer_addr).unwrap();
                                let mut peer_addr = match peer_addr.ip() {
                                    IpAddr::V4(ip) => ip.octets().to_vec(),
                                    IpAddr::V6(ip) => {
                                        panic!("Protocol currently doesn't support ipv6 :(")
                                    }
                                };
                                peer_addr.append(&mut data);

                                let len = len + 6;

                                debug!("Network BROADCAST received!");
                                ret_nodeout_send.send(RunOp::Broadcast(data.clone()));
                                ipc_nodeout_send.send(RunOp::Broadcast(data));
                            }
                            _ => {
                                println!("??? Unknown OpCode ???: ({:?}, Length: {:?})", data, len);
                            }
                        }
                    }
                    Err(ref e) if e.kind() == ErrorKind::WouldBlock => {}
                    Err(ref e) if e.kind() == ErrorKind::UnexpectedEof => {
                        println!("Peer has disconnected!");
                        let host_sock = recv.peer_addr().unwrap();

                        peers_to_remove.insert(host_sock.clone());
                        peers.write().unwrap().remove(&host_sock);
                        ping_status.write().unwrap().remove(&host_sock);
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
                    send_command(NetOp::Ping, 0, &vec![], &mut write.lock().unwrap());
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
                    peers_to_remove.insert(host_sock.clone());
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
                    println!("iter2");
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
                Ok(RunOp::Broadcast(data)) => {
                    for (_, (read, write)) in peers.read().unwrap().iter() {
                        send_command(
                            NetOp::Broadcast,
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
                Ok(RunOp::Broadcast(data)) => {
                    for (_, (read, write)) in peers.read().unwrap().iter() {
                        send_command(
                            NetOp::Broadcast,
                            data.len().try_into().unwrap(),
                            &data,
                            &mut write.lock().unwrap(),
                        );
                    }
                }
                Ok(_) => panic!("Shouldn't be receiving a anything else"),
                Err(_) => {}
            }

            thread::sleep(Duration::from_secs(NET_DELAY as u64));
        }
    });
}

pub fn start_ipc_thread(
    mut peers: Peers,
    mut ping_status: Pings,
    mut peer_list: PeerList,
    mut ipc_nodein_send: Sender<Command>,
    mut ipc_nodeout_recv: Receiver<Command>,
) -> JoinHandle<()> {
    return thread::spawn(move || {});
}

pub fn start_accept_thread(
    mut peers: Peers,
    mut ping_status: Pings,
    mut listener: TcpListener,
) -> JoinHandle<()> {
    return thread::spawn(move || accept_connections(ping_status, peers, listener));
}
