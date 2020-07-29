#![allow(warnings)]

extern crate argparse;
use crate::threads::RunOp;
use argparse::{ArgumentParser, Store, StoreFalse, StoreOption, StoreTrue};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Result;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};
use std::{thread, time};

//Import some functions in the other files
pub mod tak_net;
pub mod tests;
pub mod threads;
use log::{debug, info};
use log::{Level, Metadata, Record};
use log::{LevelFilter, SetLoggerError};
use tak_net::connect;
use threads::{start_accept_thread, start_network_thread, Command};

struct SimpleLogger;

impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        false
    }

    fn log(&self, record: &Record) {
        let since_epoch = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        println!(
            "[{:?} | {}]    {}",
            since_epoch,
            record.level(),
            record.args(),
        );
    }

    fn flush(&self) {}
}

static LOGGER: SimpleLogger = SimpleLogger;
pub fn init_logger() -> std::result::Result<(), SetLoggerError> {
    log::set_logger(&LOGGER).map(|()| log::set_max_level(LevelFilter::Debug))
}

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

//use net::{recv_command, send_command};

fn main() -> Result<()> {
    /*
     * Begin parsing the arguments here
     * available arguments...
     * --unpnp, -d, -t, -net_debug, --punch, --binding_ip, --connecting_ip, --punch_ip,
     * --delay, -p
     *
     */

    //IP to bind/advertise
    let mut binding_ip = "0.0.0.0:0".to_string();
    //IP of intro node
    let mut connecting_ip = "0.0.0.0:0".to_string();
    //IP of punch server
    let mut punch_ip = "0.0.0.0:0".to_string();

    //Delay each the main event loop
    let mut delay = "0".to_string();

    let mut project_dir = ".".to_string();
    //"0.0.0.0:0";
    let mut unpnp = false;
    let mut punch = false;
    let mut debug = false;
    let mut net_debug = false;
    let mut test = false;

    {
        let mut parser = ArgumentParser::new();
        parser.set_description("Takeetoe Node");

        parser.refer(&mut unpnp).add_option(
            &["--upnp"],
            StoreTrue,
            "Use unpnp to automatically port forward (router must support it)",
        );
        parser.refer(&mut debug).add_option(
            &["-d", "--debug"],
            StoreTrue,
            "Enable the debugging shell",
        );
        parser
            .refer(&mut test)
            .add_option(&["-t", "--test"], StoreTrue, "Test");
        parser.refer(&mut net_debug).add_option(
            &["-n", "--net_debug"],
            StoreTrue,
            "Enable network debugging shell",
        );
        parser.refer(&mut punch)
            .add_option(&["--punch"], StoreTrue, "Use TCP Hole Punching. If connecting to another 'punch' intro node 'punch_ip' needs to be set. Otherwise uses 'connecting_ip' as 'punch_ip' instead.");
        parser.refer(&mut binding_ip).add_option(
            &["--binding_ip"],
            Store,
            "The ip:port to bind to",
        );
        parser.refer(&mut connecting_ip).add_option(
            &["--connecting_ip"],
            Store,
            "The tcp server:port to connect to",
        );
        parser
            .refer(&mut punch_ip)
            .add_option(&["--punch_ip"], Store, "The punch server to use");
        parser.refer(&mut delay).add_option(
            &["--delay"],
            Store,
            "Sets the delay of the main event loop",
        );

        parser.refer(&mut project_dir).add_option(
            &["-p", "--project_dir"],
            Store,
            "The project directory to syncronize",
        );

        parser.parse_args_or_exit();
    }

    init_logger();

    //In the case that the node is run as a binary we can discard all of the channel handles and
    //extra data structures
    let ((net_handle, accept_handle, ipc_handle), (_, _, _), (_, _)) =
        start_node(&connecting_ip, &binding_ip, debug)?;
    accept_handle.join();
    net_handle.join();

    return Ok(());
}

//Initialize the threads and data-structures here
pub fn start_node(
    connecting_ip: &str,
    binding_ip: &str,
    debug: bool,
) -> Result<(
    (JoinHandle<()>, JoinHandle<()>, JoinHandle<()>),
    (Peers, PeerList, Pings),
    (Sender<Command>, Receiver<Command>),
)> {
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

    let mut peers: Peers = Arc::new(RwLock::new(HashMap::new()));
    let mut peer_list: PeerList = Arc::new(RwLock::new(HashMap::new()));
    let mut ping_status: Pings = Arc::new(RwLock::new(HashMap::new()));

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
        tak_net::connect(
            &connecting_ip,
            &binding_ip,
            peers.clone(),
            peer_list.clone(),
            ping_status.clone(),
            ret_nodeout_send.clone(),
            ipc_nodeout_send.clone(),
        );
    }

    if debug {
        info!("<<< Starting debug shell >>>");
        let peers = peers.clone();
        let peer_list = peer_list.clone();
        let pings = ping_status.clone();

        thread::spawn(move || loop {
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
        });
    }

    //The 'node' is the network thread
    //therefore it is the thread that...
    // > empties ret_nodein_out
    // > populates ret_nodeout_in
    let net_handle = threads::start_network_thread(
        peers.clone(),
        peer_list.clone(),
        ping_status.clone(),
        ret_nodein_recv,
        ipc_nodein_recv,
        ret_nodeout_send,
        ipc_nodeout_send,
    );
    let accept_handle = threads::start_accept_thread(peers.clone(), ping_status.clone(), listener);
    let ipc_handle = threads::start_ipc_thread(
        peers.clone(),
        ping_status.clone(),
        peer_list.clone(),
        ipc_nodein_send,
        ipc_nodeout_recv,
    );

    return Ok((
        (net_handle, accept_handle, ipc_handle),
        (peers.clone(), peer_list.clone(), ping_status.clone()),
        (ret_nodein_send, ret_nodeout_recv),
    ));
}
