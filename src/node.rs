use crate::tak_net::{recv_command, send_command, NetOp};
use crate::tests::NodeState;
use crate::threads;
use crate::threads::{Command, RunOp};
use log::{debug, error, info};
use std::collections::BTreeSet;
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Result;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::str::FromStr;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime};
use stoppable_thread::StoppableHandle;

/*
 * JUST A HEADS UP
 * ===============
 * 'Peers' is a maping of the host-to-peer-addr to IN and OUT streams to a peer
 * 'PeerList' is a maping of the host-to-peer-addr to its net_address
 * They are very similar, and although they are often modified together, they are technically
 * independent
 * This is because Peers can connect to the network before they advertise their address
 * TODO: make this official. call them 'leechers'
 */
pub type Peers = Arc<RwLock<HashMap<SocketAddr, (Mutex<TcpStream>, Mutex<TcpStream>)>>>;
pub type PeerList = Arc<RwLock<HashMap<SocketAddr, SocketAddr>>>;
pub type Pings = Arc<RwLock<HashMap<SocketAddr, (u8, SystemTime)>>>;
pub type NodeHandles = (
    StoppableHandle<()>,
    StoppableHandle<()>,
    StoppableHandle<()>,
);

pub type NodeStates = (Peers, PeerList, Pings);
pub type NodeIn = Sender<Command>;
pub type NodeOut = Receiver<Command>;

pub struct Node {
    peers: Peers,
    peer_list: PeerList,
    pings: Pings,
    threads: Option<NodeHandles>,
    input: Option<NodeIn>,
    output: Option<NodeOut>,
}

impl Node {
    pub fn new() -> Node {
        return Node {
            peers: Arc::new(RwLock::new(HashMap::new())),
            peer_list: Arc::new(RwLock::new(HashMap::new())),
            pings: Arc::new(RwLock::new(HashMap::new())),
            threads: None,
            input: None,
            output: None,
        };
    }

    pub fn output(&self) -> &NodeOut {
        if self.output.is_none() {
            panic!("Node not initialized?");
        }

        return self.output.as_ref().unwrap();
    }

    pub fn input(&self) -> &NodeIn {
        if self.input.is_none() {
            panic!("Node not initialized?");
        }

        return self.input.as_ref().unwrap();
    }

    //Called by the binary. Don't expect to every return from this
    pub fn wait(mut self) {
        let threads = self.threads.unwrap();
        threads.0.join();
        threads.1.join();
        threads.2.join();
    }

    pub fn get_peers_arc(&self) -> Peers {
        return Arc::clone(&self.peers);
    }

    pub fn get_peer_list_arc(&self) -> PeerList {
        return Arc::clone(&self.peer_list);
    }

    pub fn get_pings_arc(&self) -> Pings {
        return Arc::clone(&self.pings);
    }

    //Initialize the threads and data-structures here
    pub fn start(&mut self, connecting_ip: &str, binding_ip: &str, debug: bool) -> Result<()> {
        info!("Starting Takeetoe node...");

        /* Data Structures
         * ===============
         * 'files' | maps network path to host path and last saved conten | <net_path, (host_path, content)>
         * 'hashes' | save the last computed directory hashes | (proj_hash, file_hash)
         * 'version_history' | map commit hash to path and next commit | <from_commit, (path, to_commi)
         *
         * 'peers' | maps the host's perception of a socket's address to the node's (read_stream,
         * write_stream, advertised_addr)
         *
         * 'ping_status' | keep track of the last responses from a node
         *
         *
        //Ping Table format
        //=================
        //HashMap<host_socket, (socket_status, last_update)>
        //
        //Socket Status
        //==============
        //0 = disconnected. Needs to be collected
        //1 = alive
        //2 = pending ping
         * */

        /*let mut peers: Peers = Arc::new(RwLock::new(HashMap::new()));
        let mut peer_list: PeerList = Arc::new(RwLock::new(HashMap::new()));
        let mut ping_status: Pings = Arc::new(RwLock::new(HashMap::new()));*/

        /* ==========================================================================
         * || Takeetoe Node Overview | Andros Yang        Last Updated: 7/18/2020  ||
         * ==========================================================================
         * > uses a structure hash and file content hash to verify version integrity
         * > runs a seperate thread for the network event loop and the file IO
         *      > remember that all incoming and outgoing network events must go through the event loop
         *
         * > network protocol considers a single takeetoe client a 'node'
         *      > a group of nodes is a network
         *      > a node can only join a network if its file and content hash are the same
         *      > a single node is still a network
         *
         * > network addresses are referenced in variable names in a variety of ways
         *      > host_addr : refers to the TcpStream's negotiated address with the other side
         *      > peer_addr : referes to the address that a Takeetoe protocol advertises/allows other
         *      nodes to connect
         *
         * > file paths are also given specific variable names
         *      > host_path : refers to the file in the relevant OS file path format
         *      > net_path : a backslash-only poisx-like format that referes to how a takeetoe network
         *      ids a file
         *
         * > TODO: as of now there are plans to integrated UNPnP and TCP syncronized hole-punching
         * through a 3rd party server (punch.rs)
         *
         * */

        //THREAD CHANNEL CONNECTIONS
        //
        //    <MAIN>
        //       |
        //       |
        //       |
        //       |
        //       |__________
        //       |         |
        //     <NET><ACC><IPC>

        //Initialize the channels
        //Return channels to interface with the node for other Rust code
        //If we create mappings to other languages we'll need this
        //ALSO: remember that channels are unidirectional, not bi directional
        let (ret_nodein_send, ret_nodein_recv): (Sender<Command>, Receiver<Command>) =
            std::sync::mpsc::channel();
        let (ret_nodeout_send, ret_nodeout_recv): (Sender<Command>, Receiver<Command>) =
            std::sync::mpsc::channel();

        let (ipc_nodein_send, ipc_nodein_recv): (Sender<Command>, Receiver<Command>) =
            std::sync::mpsc::channel();
        let (ipc_nodeout_send, ipc_nodeout_recv): (Sender<Command>, Receiver<Command>) =
            std::sync::mpsc::channel();

        //Calculate the current directory hash
        //let (proj_hash, file_hash) =
        //    { get_directory_hash(project_dir.clone(), &mut files.write().unwrap(), true) };
        //hashes.write().unwrap().0 = proj_hash.clone();
        //hashes.write().unwrap().1 = file_hash.clone();

        //setup the channels for multithreaded communication
        //let (file_to_net_in, file_to_net_out) = std::sync::mpsc::channel();
        //let (net_to_file_in, net_to_file_out) = std::sync::mpsc::channel();

        //threads::start_file_thread();
        //Start the server
        let mut listener = TcpListener::bind(binding_ip)?;

        if connecting_ip != "0.0.0.0:0" {
            self.connect(
                connecting_ip,
                binding_ip,
                self.peers.clone(),
                self.peer_list.clone(),
                self.pings.clone(),
                ret_nodeout_send.clone(),
                ipc_nodeout_send.clone(),
            );
        }

        if debug {
            info!("<<< Starting debug shell >>>");
            let peers = self.peers.clone();
            let peer_list = self.peer_list.clone();
            let pings = self.pings.clone();

            stoppable_thread::spawn(move |stopped| {
                while !stopped.get() {
                    let mut buffer = String::new();
                    let stdin = std::io::stdin();
                    stdin.read_line(&mut buffer);
                    let splits = buffer.trim().split(" ").collect::<Vec<&str>>();

                    //Stdin appends a newline
                    //danielnil.com/rust_tip_compairing_strings
                    if splits[0] == "peer_list" {
                        println!("!!!Network Peers!!!");

                        println!("{0: <20} | {1: <20}", "host_address", "net_address");

                        for (host_addr, net_addr) in peer_list.read().unwrap().iter() {
                            println!("{0: <20} | {1: <20}", host_addr, net_addr);
                        }
                    } else if splits[0] == "ping" {
                        println!("!!!PING STATUS!!!");
                        println!("Now: {:?}", SystemTime::now());

                        println!(
                            "{0: <10} | {1: <20} | {2: <10}",
                            "elapsed", "host", "status",
                        );

                        for (host, (status, last_update)) in pings.read().unwrap().iter() {
                            println!(
                                "{0: <10} | {1: <20} | {2: <10}",
                                last_update.elapsed().unwrap().as_secs(),
                                host,
                                status,
                            );
                        }
                    } else if splits[0] == "host_peers" {
                        println!("!!!host_peers!!!");
                        println!("Length = {}", peer_list.read().unwrap().len());
                        println!("{:?}", peers);
                    } else {
                        println!("!!! INVALID COMMAND !!!");
                    }
                }
            });
        }

        //The 'node' is the network thread
        //therefore it is the thread that...
        // > empties ret_nodein_out
        // > populates ret_nodeout_in
        let net_handle = threads::start_network_thread(
            self.peers.clone(),
            self.peer_list.clone(),
            self.pings.clone(),
            ret_nodein_recv,
            ipc_nodein_recv,
            ret_nodeout_send,
            ipc_nodeout_send,
        );
        let accept_handle =
            threads::start_accept_thread(self.peers.clone(), self.pings.clone(), listener);
        let ipc_handle = threads::start_ipc_thread(
            "4269".to_string(),
            self.peers.clone(),
            self.pings.clone(),
            self.peer_list.clone(),
            ipc_nodein_send,
            ipc_nodeout_recv,
        );

        self.threads = Some((net_handle, accept_handle, ipc_handle));
        self.input = Some(ret_nodein_send);
        self.output = Some(ret_nodeout_recv);

        return Ok(());

        /*return Ok((
            (net_handle, accept_handle, ipc_handle),
            (
                self.peers.clone(),
                self.peer_list.clone(),
                self.pings.clone(),
            ),
            (ret_nodein_send, ret_nodeout_recv),
        ));*/
    }

    pub fn get_state(&self) -> NodeState {
        let peer_list = self
            .get_peer_list_arc()
            .read()
            .unwrap()
            .iter()
            .map(|(x, y)| y.to_string())
            .collect::<BTreeSet<String>>()
            .iter()
            .map(|x| x.clone())
            .collect::<Vec<String>>();
        let peers_count = self.get_peers_arc().read().unwrap().len();
        let pings_count = self.get_pings_arc().read().unwrap().len();

        //Returns a new struct representing a snapshot of the Node's current state
        NodeState {
            peer_list,
            peers_count,
            pings_count,
        }
    }

    pub fn stop(mut self) {
        if self.threads.is_none() {
            panic!("Wtf node not started");
        }

        let threads = self.threads.unwrap();
        threads.0.stop().join();
        threads.1.stop().join();
        threads.2.stop().join();
    }

    fn connect(
        &self,
        connecting_ip: &str,
        binding_ip: &str,
        mut peers: Peers,
        mut peer_list: PeerList,
        mut pings: Pings,
        ret_nodeout_send: Sender<RunOp>,
        ipc_nodeout_send: Sender<RunOp>,
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
        send_command(NetOp::Intro as u8, 6, &ip, &mut stream);
        //Intro peer responds with peer list
        let (mut res_op, mut res_len, mut res_data) = recv_command(&mut stream, true).unwrap();

        let mut peer_data = res_data.clone();
        let host_addr = stream.peer_addr().unwrap();
        debug!("Intro Response: {:?}", (res_op, res_len, res_data));

        //TODO: the connect method doesn't handle the possiblity that the peer goes down
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

        ret_nodeout_send.send(RunOp::OnJoin(SocketAddr::from_str(&connecting_ip).unwrap()));
        ipc_nodeout_send.send(RunOp::OnJoin(SocketAddr::from_str(&connecting_ip).unwrap()));

        debug!("Starting peer connections...");
        // Connect to each of the specified peers
        for start in (0..res_len.into()).step_by(6) {
            //https://stackoverflow.com/questions/50243866/how-do-i-convert-two-u8-primitives-into-a-u16-primitive?noredirect=1&lq=1
            let port_number: u16 =
                ((peer_data[start + 4] as u16) << 8) | peer_data[start + 5] as u16;
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
            send_command(NetOp::Ad as u8, 6, &ip, &mut stream);

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

            peer_list
                .write()
                .unwrap()
                .insert(host_addr, peer_addr.clone());
            ret_nodeout_send.send(RunOp::OnJoin(peer_addr));
            ipc_nodeout_send.send(RunOp::OnJoin(peer_addr));
            //println!("Successfully connected to Peer: {:?}", peer_addr);
        }

        debug!("Connected to all peers!");
        return Ok(());
    }
}
