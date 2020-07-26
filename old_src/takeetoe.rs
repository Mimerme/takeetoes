extern crate argparse;
extern crate igd;
#[macro_use]
extern crate derive_new;
extern crate diffy;
extern crate gitignore;
extern crate hex_string;

use argparse::{ArgumentParser, Store, StoreFalse, StoreOption, StoreTrue};
use sha2::{Digest, Sha256, Sha512};
use std::boxed::Box;
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Result;
use std::io::{self, Read, Write};
use std::io::{Error, ErrorKind};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime};
use std::{thread, time};
use walkdir::WalkDir;
//Some convenience functions for reading and writting to files
use diffy::create_patch;
use hex_string::HexString;
use std::convert::TryInto;
use std::fs::{metadata, File};
use std::fs::{read_to_string, write};
use std::io::BufReader;

//Import some functions in the other files
pub mod tak_net;
pub mod threads;
use tak_net::{recv_command, send_command};
use threads::{start_file_thread, start_network_thread};

//A test for difference matching
fn test_owo() {
    println!("OBA<A");
    //let (tx, rx) = std::sync::mpsc::channel();
    let x = "HELLO";
    let y = "HELI0";
    let file_watch = "/home/mimerme/test";

    let (dist, changeset) = diff(x, y, "");
    println!("ED: {:?}", dist);
    println!("Changeset: {:?}", changeset);

    //start_file_io(tx, file_watch.to_string());

    //for res in rx {
    //    match res {
    //        Ok(event) => println!("changed: {:?}", event),
    //        Err(e) => println!("watch error: {:?}", e),
    //    }
    //}
}

//TakeetoeProtocol OpCodes
//Used connecting to a Takeetoe network
const INTRO: u8 = 0x01; //Used for introducing a new node to the network
const INTRO_RES: u8 = 0x02; //Used by network nodes to acknowledge INTRO
const AD: u8 = 0x03; //Sent by a node to advertise its server, but doesn't expect a response
const MSG: u8 = 0x04; //A test message
const PING: u8 = 0x06; //PING a node
const PONG: u8 = 0x07; //response to a PING with PONG

//File IO OpCodes
//len = 128, <PROJ>(64) <FILE>(64)
const PROJ: u8 = 0x08; //Used for verifying project structures and file contents
const PROJ_VER: u8 = 0x09; //Used for responding to project and file verifications

//len = path.len() | data=net_path
const START_SYNC: u8 = 0x10; //Specify which file to syncronize

//NOTE: conflict resolution is handled by the clients and a voting system will likely be created to
//resolve them (automatically and manually)

//len = 2 + line.len() (0-253) | data= start_index,end_index,string
//NOTE: This will likely be able to handle at least a full line since most programmers don't leave
//their lines of 200 characters
const SYNC_SAME: u8 = 0x11; //Send the data to syncronize
const SYNC_REM: u8 = 0x12; //Send the data to syncronize
const SYNC_ADD: u8 = 0x14; //Send the data to syncronize

//Probably uneeded
//len = 2 | data = start_index,end_index
//const REM_DAT : u8 = 0x12;      //Remove data to syncronize

const STOP_SYNC: u8 = 0x13; //Alerts a peer node to stop listening for syncronization packets

const PATCH: u8 = 0x69;

//Some Test Commands
//cargo run --bin node -- --debug --delay 10 -p ~/test --binding_ip 127.0.0.1:6969
//cargo run --bin node -- --debug --delay 10 -p ~/test --binding_ip 127.0.0.1:4242 --connecting_ip 127.0.0.1:6969
//cargo run --bin node -- --debug --delay 10 -p ~/test --binding_ip 127.0.0.1:5050 --connecting_ip 127.0.0.1:6969
//
//Debug Shell Commands
//'ping' - displays the last known ping information to each peer
//'peers' - returns the host_socket_addr -> net_socket_addr mappings for each peer
//'peer_ads' -> similar/same things as 'peers' now. Used to only return the net_socket_addresses

//Program Architecture
// ===COMMAND FORMAT===
// OpCode (1 byte) | Data Length <in bytes> (1 byte) | Data

//
// send_command() -> give an opcode, datalength, and data write the bytes to a TcpStream
// recv_command() -> given a TcpStream read a command from it (so at least 2 bytes + data length)
//                   in either blocking or non-blocking mode
// main() -> process program arguments
//        -> port forward if UNPnP
//        -> create the main state of the Takeetoe node (Composed of Threads Safe (Sync/Send)) stuff
//          -> List<Peer(in,out)>
//          -> Advertising Peers
//        -> Calculate the file & proj hash of the project directory
//        -> connect to node if 'conecting_ip'
//          -> ask intro node for peer list with INTRO
//          -> intro node sends peer list with INTRO_RES
//          -> connect to each peer in list with INTRO
//              -> save each peer to 'Advertising Peers' as well
//          -> discard the intro node response in INTRO_RES
//          -> send PROJ to 'connecting_ip'
//          -> verify that PROJ_VER returns 1
//          -> add Peer(in,out) to list
//        -> if 'debug' start a debug shell that can interact with the node state on a seperate thread
//        -> Start a new event loop on a seperate thread that goes over List<Peer> and handles new data as it comes through peer.in
//          -> TakeetoeProtocol
//        -> Start a new thread that periodically pings the advertised peers
//        -> On the main thread, start a loop that handles new connections and adds them to
//           List<Peer>

//Returns the structure and content hash
//2 seperate hashes, one represents the directory structure, the other the file contents
fn get_directory_hash(
    project_dir: String,
    files: &mut HashMap<String, (String, String)>,
    output_files: bool,
) -> (Vec<u8>, Vec<u8>) {
    let mut proj_hasher = Sha512::new();
    let mut file_hasher = Sha512::new();

    //Load/Create the .gitignore file
    let ignore = format!("{}/.gitignore", project_dir.clone());
    let gitignore_path = Path::new(&ignore);

    if !Path::exists(gitignore_path) {
        File::create(gitignore_path);
    }

    let git_ignore = gitignore::File::new(gitignore_path).unwrap();
    let project_dir = &project_dir.clone();

    println!("Project Directory: {:?}", project_dir);
    println!("Loading the .gitignore file from: {:?}", gitignore_path);

    //Get all the files that are not excluded by the .gitignore
    let mut proj_iter = git_ignore.included_files().unwrap();
    proj_iter.sort_by(|a, b| a.file_name().unwrap().cmp(b.file_name().unwrap()));

    println!("{:?}", proj_iter);

    //for entry in WalkDir::new(&project_dir).sort_by(|a,b| a.file_name().cmp(b.file_name())) {
    for entry in proj_iter.iter() {
        //Remove the beginning /home/user/... stuff so project structures are the same across machines
        //
        //host_path refers to the local machine's path to the file
        //net_path refers to the p2p network's identifier for the file
        let host_path = entry.as_path().clone();
        let net_path = host_path
            .strip_prefix(Path::new(project_dir))
            .expect("Error stripping the prefix of a project entry");
        println!("Potential Network Entry: {:?}", &net_path);

        //Update the project structure hash
        proj_hasher.update(net_path.to_str().unwrap().as_bytes());

        if metadata(host_path.clone()).unwrap().is_file() {
            let file_contents = read_to_string(host_path.clone()).unwrap();
            file_hasher.update(file_contents.as_bytes());

            if output_files {
                files.insert(
                    net_path.to_str().unwrap().to_string(),
                    (host_path.to_str().unwrap().to_string(), file_contents),
                );
            }
        }
    }
    //Generate the hashes and return them
    let proj_hash = proj_hasher.finalize();
    let file_hash = file_hasher.finalize();
    return (proj_hash.to_vec(), file_hash.to_vec());
}

fn main() -> Result<()> {
    let mut connections: Vec<TcpStream> = vec![];

    //IP to bind/advertise
    let mut binding_ip = "0.0.0.0:0".to_string();
    //IP of intro node
    let mut connecting_ip = "0.0.0.0:0".to_string();
    //IP of punch server
    let mut punch_ip = "0.0.0.0:0".to_string();

    //Delay each connection's event loop
    let mut conn_loop_delay = "0".to_string();
    //Delay each the main event loop
    let mut event_loop_delay = "0".to_string();
    //Delay for both conn_loop_delay and event_loop_delay
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
        parser.refer(&mut conn_loop_delay).add_option(
            &["--conn_delay"],
            Store,
            "The delay for each connection's event loop",
        );
        parser.refer(&mut event_loop_delay).add_option(
            &["--event_delay"],
            Store,
            "The delay for the main event loop",
        );
        parser.refer(&mut delay).add_option(
            &["--delay"],
            Store,
            "Sets the value of '--event_delay' and '--conn_delay'",
        );

        parser.refer(&mut project_dir).add_option(
            &["-p", "--project_dir"],
            Store,
            "The project directory to syncronize",
        );

        parser.parse_args_or_exit();
    }

    if test {
        test_owo();
        panic!("test");
    }

    if delay != "0" {
        conn_loop_delay = delay.clone();
        event_loop_delay = delay.clone();
    }

    let conn_loop_delay = time::Duration::from_millis(conn_loop_delay.parse::<u64>().unwrap());
    let event_loop_delay = time::Duration::from_millis(event_loop_delay.parse::<u64>().unwrap());

    if debug {
        println!("Binding Addr: {}", &binding_ip);
        println!("Connecting Addr: {}", &connecting_ip);
    }

    //unpnp code
    //TODO: doesn't work on WSL as of now. Try native later
    if unpnp {
        let splits: Vec<&str> = binding_ip.split(':').collect();
        let local_addr = splits[0];
        let local_port = splits[1];
        let local_port = local_port.parse::<u16>().unwrap();

        //let gateway = igd::search_gateway(Default::default()).unwrap();

        match igd::search_gateway(igd::SearchOptions::default()) {
            Err(ref err) => println!("Error1: {}", err),
            Ok(gateway) => {
                let local_addr = local_addr.parse::<Ipv4Addr>().unwrap();
                let local_addr = SocketAddrV4::new(local_addr, local_port);

                match gateway.add_port(
                    igd::PortMappingProtocol::TCP,
                    7890,
                    local_addr,
                    60,
                    "add_port example",
                ) {
                    Err(ref err) => {
                        println!("There was an error! {}", err);
                    }
                    Ok(()) => {
                        println!("Added port");
                    }
                }
            }
        }
    }

    //===FILE DATA STRUCTURES===
    //We verify project structure and file integrity with two seperate hashes
    let mut files: Arc<RwLock<HashMap<String, (String, String)>>> =
        Arc::new(RwLock::new(HashMap::new()));
    let mut hashes: Arc<RwLock<(Vec<u8>, Vec<u8>)>> = Arc::new(RwLock::new((vec![], vec![])));
    //                                           K: file_hash (filename+contents), V: (Contents,
    //                                           next_hash [null = ""])
    let mut version_history: Arc<RwLock<HashMap<String, (String, String)>>> =
        Arc::new(RwLock::new(HashMap::new()));

    //===NETWORK DATA STRUCTURES===
    //Peer Vec Format
    //(read: TcpStream, write: TcpStream, advertied_ip: SocketAddr)
    let mut peers: Arc<RwLock<HashMap<SocketAddr, (Mutex<TcpStream>, Mutex<TcpStream>)>>> =
        Arc::new(RwLock::new(HashMap::new()));
    //<K,V> = <host_socket_add, net_socket_addr>
    let mut peer_ads: Arc<RwLock<HashMap<SocketAddr, SocketAddr>>> =
        Arc::new(RwLock::new(HashMap::new()));

    //Ping Table format
    //=================
    //HashMap<host_socket, (socket_status, last_update)>
    //
    //Socket Status
    //==============
    //0 = disconnected. Needs to be collected
    //1 = alive
    //2 = pending ping
    let ping_status: Arc<RwLock<HashMap<SocketAddr, (u8, SystemTime)>>> =
        Arc::new(RwLock::new(HashMap::new()));
    let ping_status_clone: Arc<RwLock<HashMap<SocketAddr, (u8, SystemTime)>>> =
        Arc::clone(&ping_status);
    let ping_status_clone2: Arc<RwLock<HashMap<SocketAddr, (u8, SystemTime)>>> =
        Arc::clone(&ping_status);
    let ping_peer_ads = Arc::clone(&peer_ads);

    //Calculate the directory hash
    //wrap this is brackets to ensure that files.write() lock gets dropped after
    let (proj_hash, file_hash) =
        { get_directory_hash(project_dir.clone(), &mut files.write().unwrap(), true) };
    hashes.write().unwrap().0 = proj_hash.clone();
    hashes.write().unwrap().1 = file_hash.clone();

    if debug {
        println!("PROJ HASH: {:?}", HexString::from_bytes(&proj_hash));
        println!("FILE HASH: {:?}", HexString::from_bytes(&file_hash));
    }

    //Send outbound communications for the IO thread
    let pd_clone = project_dir.clone();
    let (event_send, event_recv) = mpsc::channel();
    let (change_send, change_recv) = mpsc::channel();

    start_file_thread();

    //Network Protocol
    //OpCode | Data Length | Data
    //1 byte | 1 byte      | <Data Length> byte

    // |__new peer__|         |___intro peer___|
    // |            |\        |
    //              | \       |
    //              |INTRO    |
    //   1 (1b) | 6 (1b) | advertised_ ip (6b)
    //              |     \   |
    //              |      \  |
    //              |       \ |
    //              |        \|
    //              |         |
    //              |        /|
    //              |INTRO_RES|
    //   2 (1b) | 0..252 (1b) | peer_ip : peer_port + ... (0..252b)
    //'new_peer' now connects to each peer, thus adding itself to each node's peer_list
    //              |   /     |
    //              |  /      |
    //              | /       |
    //              |/        |
    //  <----------AD to all peers--------->
    //              |         |
    //              |         |
    //  <------- PROJ to all peers-------->
    //   >-------PROJ_VER from all peers--------<
    //              |         |
    //              |         |
    //              |         |
    //              |         |    //                        |

    //If a connecting_ip is specified then establish a connection and retrieve the peer list
    if connecting_ip != "0.0.0.0:0" {
        if debug {
            println!("Connecting to {}...", connecting_ip);
        }
        //Connect to the stream
        let mut stream = TcpStream::connect(connecting_ip.clone())?;
        let mut stream_clone = stream.try_clone().unwrap();
        //Prepare a 2 byte buffer for the network header

        //Creating the 'advertised ip' by converting the cli argument into bytes
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
        //[opcode (1), data_length (6), ip : port]
        if debug {
            println!("Introducing IP {:?}", binding_sock);
        }
        send_command(INTRO, 6, &ip, &mut stream);
        //Intro peer responds with peer list
        let (mut res_op, mut res_len, mut res_data) = recv_command(&mut stream, true).unwrap();

        let mut peer_data = res_data.clone();
        if debug {
            println!("Response: {:?}", (res_op, res_len, res_data));
        }

        //Peer List network format is in:
        //ip1 (4 bytes) | port1 (2 bytes) | ip2 ...

        peer_ads.write().unwrap().insert(
            stream.peer_addr().unwrap(),
            SocketAddr::from_str(&connecting_ip).unwrap(),
        );
        let proj_hash: &Vec<u8> = &hashes.read().unwrap().0.clone();
        let file_hash: &Vec<u8> = &hashes.read().unwrap().1.clone();

        println!(
            "Hash Valid?: {:?}",
            verify_file_hash(proj_hash, file_hash, &mut stream)
        );

        if verify_file_hash(proj_hash, file_hash, &mut stream) == false {
            panic!("Different filehash than into node");
        }

        //Add this initial connection to the peer list
        ping_status
            .write()
            .unwrap()
            .insert(stream.peer_addr().unwrap(), (1, SystemTime::now()));
        peers.write().unwrap().insert(
            stream.peer_addr().unwrap(),
            (Mutex::new(stream_clone), Mutex::new(stream)),
        );

        println!("Starting peer connections...");
        // Connect to each of the specified peers
        for start in (0..res_len.into()).step_by(6) {
            println!("ONE");
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

    //Start the server
    let mut listener = TcpListener::bind(binding_ip)?;
    let mut peers_clone = Arc::clone(&peers);
    let mut peers_debug = Arc::clone(&peers);
    let mut peer_ads_debug = Arc::clone(&peer_ads);

    if debug {
        println!("<<< Starting debug shell >>>");

        thread::spawn(move || loop {});
    }

    let peers_clone = Arc::clone(&peers);
    start_network_thread();

    //Accept every incomming TCP connection on the main thread
    for stream in listener.incoming() {
        println!("New connection");
        //Stream ownership is passed to the thread
        let mut tcp_connection: TcpStream = stream.unwrap();
        let peer_addr = tcp_connection.peer_addr().unwrap();

        //Add to the peer list
        ping_status
            .write()
            .unwrap()
            .insert(tcp_connection.peer_addr().unwrap(), (1, SystemTime::now()));
        peers.write().unwrap().insert(
            tcp_connection.peer_addr().unwrap(),
            (
                Mutex::new(tcp_connection.try_clone().unwrap()),
                Mutex::new(tcp_connection),
            ),
        );
    }

    return Ok(());
}
