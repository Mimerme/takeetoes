extern crate argparse;
extern crate igd;
#[macro_use]
extern crate derive_new;
extern crate gitignore;
extern crate hex_string;

use std::time::Duration;
use std::net::{TcpListener, TcpStream, Ipv4Addr, SocketAddrV4, SocketAddr, IpAddr};
use std::io::Result;
use std::io::{Error, ErrorKind};
use std::io::{self, Read, Write};
use argparse::{ArgumentParser, StoreTrue, Store, StoreOption, StoreFalse};
use std::sync::{Arc, Mutex, RwLock};
use std::sync::mpsc::{Sender, Receiver};
use std::sync::mpsc;
use std::ops::{Deref, DerefMut};
use std::collections::{HashSet, HashMap};
use std::{thread, time};
use std::boxed::Box;
use std::str::FromStr;
use walkdir::WalkDir;
use std::path::Path;
use sha2::{Sha256, Sha512, Digest};
//Some convenience functions for reading and writting to files
use std::fs::{read_to_string, write};
use std::fs::{metadata, File};
use std::io::BufReader;
use hex_string::HexString;

//TakeetoeProtocol OpCodes
const INTRO : u8 = 0x01;        //Used for introducing a new node to the network
const INTRO_RES : u8 = 0x02;    //Used by network nodes to acknowledge INTRO
const AD : u8 = 0x03;           //Sent by a node to advertise its server, but doesn't expect a response
const MSG : u8 = 0x04;          //A test message
const PING : u8 = 0x06;         //PING a node
const PONG : u8 = 0x07;         //response to a PING with PONG

//File IO OpCodes
//len = 128, <PROJ>(64) <FILE>(64)
const PROJ : u8 = 0x08;         //Used for verifying project structures and file contents
const PROJ_VER : u8 = 0x09;     //Used for responding to project and file verifications

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



fn verify_file_hash(proj_hash : &Vec<u8>, file_hash : &Vec<u8>, mut peer_stream : &mut TcpStream) -> bool{
      //Create the data vector to send
      let mut data = proj_hash.clone();
      data.extend(file_hash.clone());

      //Begin verifying the hash with the peer
      println!("Verifying hash with {:?}...", data);
      send_command(PROJ, 128, &data, &mut peer_stream);
      let (opcode, len, data) = recv_command(&mut peer_stream, true).unwrap();
      println!("PPO {:?}, {:?}, {:?}", opcode, len, data);
      
      if opcode != PROJ_VER || len != 1{
          panic!("ERR: Protocol out-of-order");
      }

      //1 = verified
      //0 = non-matching hashes
      //2 = willing to syncronize
      match data[0] {
         0 => {return false;},
         1 => {return true;},
         2 => {
            //Begin syncronization
            panic!("File syncronization needs implementing");
         },
        _ => {
            panic!("Protocol Err");
        }
      }
}



//Returns the structure and content hash
fn get_directory_hash(project_dir : String, files :&mut HashMap<String, (String, String)>, output_files : bool) -> (Vec<u8>, Vec<u8>){

     let mut proj_hasher = Sha512::new();
     let mut file_hasher = Sha512::new();
     let ignore = format!("{}/.gitignore", project_dir.clone());
     let gitignore_path = Path::new(&ignore);

     if !Path::exists(gitignore_path){
        File::create(gitignore_path); 
     }
        
     let git_ignore = gitignore::File::new(gitignore_path).unwrap();
     let project_dir = &project_dir.clone();

     println!("Project Directory: {:?}", project_dir);
     println!("Loading the .gitignore file from: {:?}", gitignore_path);

     //Get all the files that are not excluded by the .gitignore
     let mut proj_iter = git_ignore.included_files().unwrap();
     proj_iter.sort_by(|a, b| a.file_name().unwrap().cmp(b.file_name().unwrap()));

     //for entry in WalkDir::new(&project_dir).sort_by(|a,b| a.file_name().cmp(b.file_name())) {
     for entry in proj_iter.iter(){
         //Remove the beginning /home/user/... stuff so project structures are the same across machines
         //
         //host_path refers to the local machine's path to the file
         //net_path refers to the p2p network's identifier for the file
         let host_path = entry.as_path().clone();
         let net_path = host_path.strip_prefix(Path::new(project_dir)).expect("Error stripping the prefix of a project entry");
         println!("Potential Network Entry: {:?}", &net_path);

         //Update the project structure hash
         proj_hasher.update(net_path.to_str().unwrap().as_bytes());

         if metadata(host_path.clone()).unwrap().is_file() {
            let file_contents = read_to_string(host_path.clone()).unwrap();
            file_hasher.update(file_contents.as_bytes());

            if output_files{
                files.insert(net_path.to_str().unwrap().to_string(), (host_path.to_str().unwrap().to_string(), file_contents));
            }
         }

     }
     let proj_hash = proj_hasher.finalize();
     let file_hash = file_hasher.finalize();
     return (proj_hash.to_vec(), file_hash.to_vec());
}

//Send a command to a TCPStream
fn send_command(opcode : u8, data_len : u8, data : &Vec<u8>, connection : &mut TcpStream)->Result<()>{
     let mut data = data.clone();
     //Add the headers to the outgoing data
     data.insert(0, data_len);
     data.insert(0, opcode);
     connection.write_all(&data);
     println!("Sent command: {:?} {:?} {:?}", opcode, data_len, data);
     return Ok(());
}

fn recv_command(connection : &mut TcpStream, block : bool)->Result<(u8, u8, Vec<u8>)>{
    let mut network_header = vec![0;2];
    // Keep peeking until we have a network header in the connection stream
    //println!("recv_command()");

    //println!("Loop: {:?}", block);
    //Block until an entire command is received
    if block {
        println!("Blocking...");
        connection.read_exact(&mut network_header);
        let data_len : u8 = network_header[1];
        let dl_index : usize =  data_len.into();
        let mut command = vec![0; dl_index];

        println!("Waiting for data... {:?}", dl_index);
        if data_len != 0 { 
            connection.read_exact(&mut command);
            return Ok((network_header[0], data_len, command));
        }
        else {
            return Ok((network_header[0], 0, vec![]));
        }
    }
    else {
        connection.set_nonblocking(true).expect("set_nonblocking failed");

        if connection.peek(&mut network_header)? < network_header.len() {
            return Err(Error::new(ErrorKind::WouldBlock, "would block"));
        }
        let data_len : u8 = network_header[1];
        let dl_index : usize = data_len.into();
        let mut command = vec![0; dl_index + 2];
        println!("NO BLOCK2 {}", command.len());
        if connection.peek(&mut command)? < command.len() {
            println!("YYEEET");
            return Err(Error::new(ErrorKind::WouldBlock, "would block"));
        }

        //println!("READING EXACT");
        connection.read_exact(&mut command);
        return Ok((command[0], data_len, command[2..(dl_index + 2)].to_vec()));
    }
}


fn main() -> Result<()>{
    let mut connections : Vec<TcpStream> = vec![];
    
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

    {
        let mut parser = ArgumentParser::new();
        parser.set_description("Takeetoe Node");

        parser.refer(&mut unpnp)
            .add_option(&["--upnp"], StoreTrue, "Use unpnp to automatically port forward (router must support it)");
        parser.refer(&mut debug)
            .add_option(&["-d", "--debug"], StoreTrue, "Enable the debugging shell");
        parser.refer(&mut punch)
            .add_option(&["--punch"], StoreTrue, "Use TCP Hole Punching. If connecting to another 'punch' intro node 'punch_ip' needs to be set. Otherwise uses 'connecting_ip' as 'punch_ip' instead.");
        parser.refer(&mut binding_ip)
            .add_option(&["--binding_ip"], Store, "The ip:port to bind to");
        parser.refer(&mut connecting_ip)
            .add_option(&["--connecting_ip"], Store, "The tcp server:port to connect to");
        parser.refer(&mut punch_ip)
            .add_option(&["--punch_ip"], Store, "The punch server to use");
        parser.refer(&mut conn_loop_delay)
            .add_option(&["--conn_delay"], Store, "The delay for each connection's event loop");
        parser.refer(&mut event_loop_delay)
            .add_option(&["--event_delay"], Store, "The delay for the main event loop");
         parser.refer(&mut delay)
            .add_option(&["--delay"], Store, "Sets the value of '--event_delay' and '--conn_delay'");

        parser.refer(&mut project_dir)
            .add_option(&["-p", "--project_dir"], Store, "The project directory to syncronize");
 
        parser.parse_args_or_exit();
   
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
        let splits : Vec<&str> = binding_ip.split(':').collect();
        let local_addr = splits[0];
        let local_port = splits[1];
        let local_port = local_port.parse::<u16>().unwrap();

        //let gateway = igd::search_gateway(Default::default()).unwrap();

        match igd::search_gateway(igd::SearchOptions::default()){
             Err(ref err) => println!("Error1: {}", err),
             Ok(gateway) => {
                 let local_addr = local_addr.parse::<Ipv4Addr>().unwrap();
                 let local_addr = SocketAddrV4::new(local_addr, local_port);

                 match gateway.add_port(igd::PortMappingProtocol::TCP, 7890, local_addr, 60, "add_port example"){
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

   //We verify project structure and file integrity with two seperate hashes
   let mut files : HashMap<String, (String, String)> = HashMap::new();
   let mut hashes : Arc<RwLock<(Vec<u8>, Vec<u8>)>> = Arc::new(RwLock::new((vec![], vec![])));

   //Calculate the directory hash
   let (proj_hash, file_hash) = get_directory_hash(project_dir.clone(), &mut files, true);
   hashes.write().unwrap().0 = proj_hash.clone();
   hashes.write().unwrap().1 = file_hash.clone();

   if debug {
      println!("PROJ HASH: {:?}", HexString::from_bytes(&proj_hash));
      println!("FILE HASH: {:?}", HexString::from_bytes(&file_hash));
   }

   //Start the file IO thread
   thread::spawn(move || {

   });





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


   //Peer Vec Format
   //(read: TcpStream, write: TcpStream, advertied_ip: SocketAddr)
   let mut peers : Arc<RwLock<Vec<(Mutex<TcpStream>, Mutex<TcpStream>)>>> =
       Arc::new(RwLock::new(vec![]));
   //<K,V> = <host_socket_add, net_socket_addr>
   let mut peer_ads : Arc<RwLock<HashMap<SocketAddr, SocketAddr>>> = Arc::new(RwLock::new(HashMap::new()));

   //If a connecting_ip is specified then establish a connection and retrieve the peer list
   if connecting_ip != "0.0.0.0:0" {
        if debug {println!("Connecting to {}...", connecting_ip);}
        //Connect to the stream
        let mut stream = TcpStream::connect(connecting_ip.clone())?;
        let mut stream_clone = stream.try_clone().unwrap();
        //Prepare a 2 byte buffer for the network header

        //Creating the 'advertised ip' by converting the cli argument into bytes
        let binding_sock = SocketAddr::from_str(&binding_ip).unwrap();
        let mut ip = match binding_sock.ip() {
            IpAddr::V4(ip) => ip.octets().to_vec(),
            IpAddr::V6(ip) => panic!("Protocol currently doesn't support ipv6 :(")
        };
        let port : u16 = binding_sock.port();
        ip.push((port >> 8) as u8);
        ip.push(port as u8);

        //Begin asking for the network's peer list
        //INTRO packet and an advertised peer list         
        //[opcode (1), data_length (6), ip : port]
        if debug {println!("Introducing IP {:?}", binding_sock);}
        send_command(INTRO, 6, &ip, &mut stream);
        //Intro peer responds with peer list
        let (mut res_op, mut res_len, mut res_data) = recv_command(&mut stream, true).unwrap();

        let mut peer_data = res_data.clone();
        if debug {println!("Response: {:?}", (res_op, res_len, res_data));}

        //Peer List network format is in:
        //ip1 (4 bytes) | port1 (2 bytes) | ip2 ...  
        

        peer_ads.write().unwrap().insert(stream.peer_addr().unwrap(), SocketAddr::from_str(&connecting_ip).unwrap());
        let proj_hash : &Vec<u8> = &hashes.read().unwrap().0.clone();
        let file_hash : &Vec<u8> = &hashes.read().unwrap().1.clone();


        println!("Hash Valid?: {:?}", verify_file_hash(proj_hash, file_hash, &mut stream));

        if verify_file_hash(proj_hash, file_hash, &mut stream) == false {
            panic!("Different filehash than into node");
        }

        //Add this initial connection to the peer list
        peers.write().unwrap().push((
            Mutex::new(stream_clone),
            Mutex::new(stream)
        ));

        println!("Starting peer connections...");
        // Connect to each of the specified peers
        for start in (0..res_len.into()).step_by(6) {
            println!("ONE");
            //https://stackoverflow.com/questions/50243866/how-do-i-convert-two-u8-primitives-into-a-u16-primitive?noredirect=1&lq=1
            let port_number : u16 = ((peer_data[start + 4] as u16) << 8) | peer_data[start + 5] as u16;
            let peer_addr = SocketAddr::from(([peer_data[start], peer_data[start + 1], peer_data[start + 2], peer_data[start + 3]], port_number));

            //Do not allow self referencing peers
            if binding_sock == peer_addr {
                println!("Found self referencing peers");
                continue;
            }

            //Connect to the specified peer
            println!("Peer: {:?}", peer_addr);
            let mut stream = TcpStream::connect(peer_addr).unwrap();
            
            //Clear out the response. Lol, probably should do some verification here
            //let intro_res = recv_command(&mut stream, true);
            //send_command(AD, 6, &ip, &mut stream);

            if verify_file_hash(proj_hash, file_hash, &mut stream) == false {
                panic!("Different filehash than network");
            }

            //TODO: figure out why moving this line above verify_file_hash() results in a hang
            //Send an AD command to add node to the network's peer list 
            send_command(AD, 6, &ip, &mut stream);
 

            peer_ads.write().unwrap().insert(stream.peer_addr().unwrap(), peer_addr);
            peers.write().unwrap().push((
                Mutex::new(stream.try_clone().unwrap()), 
                Mutex::new(stream),
            ));
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

        thread::spawn(move ||{
            loop {
                let mut buffer = String::new();
                let stdin = io::stdin();
                stdin.read_line(&mut buffer);
                let splits = buffer.trim().split(" ").collect::<Vec<&str>>();

                //Stdin appends a newline
                //danielnil.com/rust_tip_compairing_strings
                if splits[0] == "peer_ads" {
                    println!("Advertised Peers");
                    println!("{:?}", peer_ads_debug.read().unwrap().iter());
                }
                //Send a broadcast to all Nodes
                else if splits[0] == "msg" {
                    println!("Sending a broadcast..."); 
                    for (read, write) in peers_debug.write().unwrap().iter() {
                        let mut write = write.lock().unwrap();
                        let data = (&splits[1].as_bytes()).to_vec();
                        send_command(MSG, data.len() as u8, &data, &mut write);
                    }
                }
                else if splits[0] == "peers" {
                    println!(">>>Peer List<<<");
                    for (host_ip, net_ip) in peer_ads_debug.read().unwrap().iter() {
                        println!("Peer: {:?}, {:?}", host_ip, net_ip);
                    }
                }
            }
        });
   }


   //Start the peer watcher thread
   thread::spawn(move || {
        loop{
            for (read, write) in peers_clone.read().unwrap().iter() {
                
            }
        }        
   });


   let peers_clone = Arc::clone(&peers);
   //Thread to run the main event loop / protocol
   //This thread only deals with peers who have been learned / connected with
   thread::spawn(move || {
        let hashes = Arc::clone(&hashes);
        let hash_cache : HashSet<(Vec<u8>, Vec<u8>)> = HashSet::new();

        loop{
            //println!("{:?}", peers_clone.read().unwrap().iter().len());
            let mut peer_index = 0;
            //println!("{}", peers_clone.read().unwrap().iter().len());
            for (read, write) in peers_clone.read().unwrap().iter() {
                //send.lock().unwrap().send(b"asaaa".to_vec());
                let mut recv = read.lock().unwrap();
                match recv_command(&mut recv, false){
                    Ok((opcode, len, data)) => {
                        //Check for the opcode of the data first
                        match opcode {
                            INTRO => {
                                println!("Received an INTRO");
                                //if verbose { println!("{} requested the peer list", string_ip);}
                                
                                let peers = peers_clone.read().unwrap();
                                let wr = write;
                                let mut write = wr.lock().unwrap();
                                //if verbose { println!("{:?}", peer_ads.read().unwrap());}
                               

                                let ad_ip_data = data;
                                //Format 'peer_ads' into bytes
                                let mut data = vec![];
                                for (_, peer) in peer_ads.read().unwrap().iter() {
                                    let mut ip = match peer.ip() {
                                        IpAddr::V4(ip) => ip.octets().to_vec(),
                                        IpAddr::V6(ip) => panic!("Protocol currently doesn't support ipv6 :("),
                                    };
                                    let port : u16 = peer.port();
                                    ip.push((port >> 8) as u8);
                                    ip.push(port as u8);
                                    data.extend(ip.iter());
                                }

                                send_command(INTRO_RES, (peer_ads.read().unwrap().len() * 6) as u8,
                                &data, &mut write);
                                if debug {println!("Responded with peer list");}

                                let port_number : u16 = ((ad_ip_data[4] as u16) << 8) | ad_ip_data[5] as u16;
                                let advertised_addr = SocketAddr::from(([ad_ip_data[0],
                                        ad_ip_data[1], ad_ip_data[2], ad_ip_data[3]], port_number));

                                peer_ads.write().unwrap().insert(write.peer_addr().unwrap(), advertised_addr);
                                if debug {println!("Added {:?} to the peer_list (INTRO)", advertised_addr);}
                            },
                            AD => {
                                println!("SAVING AD");
                                let ad_ip_data = data;
                                let port_number : u16 = ((ad_ip_data[4] as u16) << 8) | ad_ip_data[5] as u16;
                                let advertised_addr = SocketAddr::from(([ad_ip_data[0],
                                        ad_ip_data[1], ad_ip_data[2], ad_ip_data[3]], port_number));

                                println!("Waiting for peer_ads");
                                //Udate the peer and advertisement list with the addr
                                peer_ads.write().unwrap().insert(write.lock().unwrap().peer_addr().unwrap(), advertised_addr.clone());

                                println!("UPDARTED!!");
                                if debug {println!("Added {:?} to the peer_list (AD)", advertised_addr);}
                            },
                            MSG => {
                                println!("MSG: {}", String::from_utf8(data.to_vec()).unwrap());
                            },
                            PING => {
                                if debug {println!("Received a PING");}
                                send_command(PONG, 0, &vec![], &mut write.lock().unwrap()); 
                                if debug {println!("Sent a PONG");}
                            }
                            PONG => {
                                if debug {println!("Receivied a PONG");}
                            },
                            PROJ => {
                                println!("Responding to project verification");
                                if data.len() != 128 {
                                    println!("ERR: Received an invalid PROJ request");
                                }
                                let req_proj = data[0..64].to_vec();
                                let req_file = data[64..128].to_vec();

                                if debug {
                                    println!("RECV PROJ: {:?}", req_proj);
                                    println!("RECV FILE: {:?}", req_file);
                                }
                                let (proj_hash, file_hash) = get_directory_hash(project_dir.clone(), &mut HashMap::new(), false);

                                //TODO: implement syncronization
                                if proj_hash == req_proj && req_file == file_hash {
                                    send_command(PROJ_VER, 1, &vec![1], &mut write.lock().unwrap());
                                }
                                else{
                                    send_command(PROJ_VER, 1, &vec![0], &mut write.lock().unwrap());
                                }

                                let mut hashes = hashes.write().unwrap();
                                hashes.0 = proj_hash;
                                hashes.1 = file_hash;
                            }
                            _ => {
                                println!("??? Unknown OpCode ???: ({:?}, Length: {:?})", data, len);
                            }
                        }



                        //println!("{:?}", data);
                    },
                    Err(ref e) if e.kind() ==  ErrorKind::WouldBlock => {},
                    Err(e) => {
                        panic!(e);
                        println!("REMOVING PERE");
                    }
                }
                peer_index += 1;
            }
            thread::sleep(event_loop_delay);
       }
   });

   //Accept every incomming TCP connection on the main thread
   for stream in listener.incoming() {
       println!("New connection");
       //Stream ownership is passed to the thread
       let mut tcp_connection: TcpStream = stream.unwrap();
       let peer_addr = tcp_connection.peer_addr().unwrap();

       //Add to the peer list
       peers.write().unwrap().push((
           Mutex::new(tcp_connection.try_clone().unwrap()), 
           Mutex::new(tcp_connection),
       ));
   }

    return Ok(());
}
