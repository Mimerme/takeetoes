extern crate argparse;
extern crate igd;
#[macro_use]
extern crate derive_new;

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
use derefable::Derefable;
use std::boxed::Box;
use std::str::FromStr;

const INTRO : u8 = 0x01;        //Used for introducing a new node to the network
const INTRO_RES : u8 = 0x02;    //Used by network nodes to acknowledge INTRO
const AD : u8 = 0x03;
const MSG : u8 = 0x04;          //A test message
const PING : u8 = 0x06;         //PING a node
const PONG : u8 = 0x07;         //response to a PING with PONG

//Program Architecture
// main() -> process program arguments 
//        -> port forward if UNPnP
//        -> create the main state of the Takeetoe node (Composed of Threads Safe (Sync/Send)) stuff
//          -> List<Peer(in,out)>
//          -> Advertising Peers
//        -> connect to node if 'conecting_ip'
//          -> ask intro node for peer list with INTRO
//          -> intro node sends peer list with INTRO_RES
//          -> connect to each peer in list with INTRO
//              -> save each peer to 'Advertising Peers' as well
//          -> discard the intro node response in INTRO_RES
//              -> spawn_connection_thread() on the connection
//              -> add Peer(in,out) to list
//        -> if 'debug' start a debug shell that can interact with the node state on a seperate thread
//        -> Start a new event loop that goes over List<Peer> and handles new data as it comes through peer.in
//
// spawn_connection_thread()
//Given a TcpStream...
//...spawn a new thread to handle reads and writes
//...returns a tuple of a Sender and Receiver to write and read to the stream
fn handle_connection(mut tcp_connection: TcpStream, sleep_dur : Duration) -> Result<(Receiver<u8>, Sender<Vec<u8>>)> 
{
    //Start a new thread to handle each TCPStream
    //Create a channel that'll be hared across multiple threads
    let (send_out, recv_out) = mpsc::channel::<Vec<u8>>();
    let (send_in, recv_in) = mpsc::channel::<u8>();
    let conn_addr = tcp_connection.peer_addr().unwrap().to_string();   

    thread::spawn(move || {
        tcp_connection.set_nonblocking(true).expect("failed to set tcp_connection as nonblocking");
        let mut byte_buff: Vec<u8> = vec![0; 1];

        //Main event loop
        //on each iteration...
        //  read in a byte, see if it matches a command, if so follow the protocol
        //  handle an incomming 'write' messages on the 'recv' end of the channel if exists
        loop {
            //Read in a byte at a time until a command is recognized
            match tcp_connection.read(&mut byte_buff) {
                //EOF: 'num_bytes_read' == 0
                Ok(num_bytes_read) => {
                    if num_bytes_read == 1 {
                        send_in.send(byte_buff[0]);
                    }
                },
                //io:ErrorKind::WouldBlock: in this case means that no bytes received
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {},
                Err(e) => panic!()
            }

            //Then handle all incomming channels and write it to the TCPStream
            match recv_out.try_recv() {
                Ok(write_data) => {tcp_connection.write(&write_data);},
                //try_recv returns an error on no data or disconnected
                //https://doc.rust-lang.org/std/sync/mpsc/enum.TryRecvError.html
                Err(e) => {}
            }

            thread::sleep(sleep_dur);

        }
    });

    return Ok((
        recv_in,
        send_out
    ));
}

//Send a command to a TCPStream
fn send_command(opcode : u8, data_len : u8, data : &Vec<u8>, connection : &mut TcpStream)->Result<()>{
     let mut data = data.clone();
     //Add the headers to the outgoing data
     data.insert(0, data_len);
     data.insert(0, opcode);
     connection.write_all(&data);
     println!("Sent command: {:?}", opcode);
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
        while connection.peek(&mut network_header).unwrap() <
            network_header.len(){println!("What");println!("f: {:?}", network_header);}
        println!("e: {:?}", network_header);
        let data_len : u8 = network_header[1];
        let dl_index : usize =  data_len.into();
        let mut command = vec![0; dl_index + 2];

        connection.read_exact(&mut command);
        let data = &command[2..(dl_index + 2)];
        return Ok((command[0], data_len, data.to_vec()));
    }
    else {
        connection.set_nonblocking(true).expect("set_nonblocking failed");
        
        if connection.peek(&mut network_header)? < network_header.len() {
            println!("ERR");
            return Err(Error::new(ErrorKind::WouldBlock, "would block"));
        }
        let data_len : u8 = network_header[1];
        let dl_index : usize = data_len.into();
        let mut command = vec![0; dl_index + 2];
        println!("NO BLOCK2");
        if connection.peek(&mut command)? < command.len() {
            println!("YYEEET");
            return Err(Error::new(ErrorKind::WouldBlock, "would block"));
        }

        println!("READING EXACT");
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
    //"0.0.0.0:0";
    let mut verbose = false;
    let mut unpnp = false;
    let mut punch = false;
    let mut debug = false;

    {
        let mut parser = ArgumentParser::new();
        parser.set_description("Takeetoe Node");


        parser.refer(&mut verbose)
            .add_option(&["-v", "--verbose"], StoreTrue, "Enable verbose logging");
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
        parser.parse_args_or_exit();
   
   }

   if delay != "0" {
       conn_loop_delay = delay.clone();
       event_loop_delay = delay.clone();
   }
    
   let conn_loop_delay = time::Duration::from_millis(conn_loop_delay.parse::<u64>().unwrap());
   let event_loop_delay = time::Duration::from_millis(event_loop_delay.parse::<u64>().unwrap());

   if verbose {
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
    //  <--------INTRO to all peers------->                                    
    //              |         |
    //              |         |
    //              |         |
    //              |         |
    //              |         |    //                        |


   //Peer Vec Format
   //(read: TcpStream, write: TcpStream, advertied_ip: SocketAddr)
   let mut peers : Arc<RwLock<Vec<(Mutex<TcpStream>, Mutex<TcpStream>)>>> =
       Arc::new(RwLock::new(vec![]));
   let mut peer_ads : Arc<RwLock<HashSet<SocketAddr>>> = Arc::new(RwLock::new(HashSet::new()));

   //If a connecting_ip is specified then establish a connection and retrieve the peer list
   if connecting_ip != "0.0.0.0:0" {
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
        let port : u16 = binding_sock.port();
        ip.push((port >> 8) as u8);
        ip.push(port as u8);

        //Begin asking for the network's peer list
        //INTRO packet and an advertised peer list         
        //[opcode (1), data_length (6), ip : port]        
        send_command(INTRO, 6, &ip, &mut stream);
        //Intro peer responds with peer list
        println!("PEE");
        let (mut res_op, mut res_len, mut res_data) = recv_command(&mut stream, true).unwrap();
        let mut peer_data = res_data.clone();
        if debug {println!("Response: {:?}", (res_op, res_len, res_data));}

        //Peer List network format is in:
        //ip1 (4 bytes) | port1 (2 bytes) | ip2 ...  
        
        //Add this initial connection to the peer list
        peers.write().unwrap().push((
            Mutex::new(stream_clone),
            Mutex::new(stream)
        ));
        peer_ads.write().unwrap().insert(SocketAddr::from_str(&connecting_ip).unwrap());
 
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
            
            //Send an INTRO command to add node to peer list 
            send_command(INTRO, 6, &ip, &mut stream);
            //Clear out the response. Lol, probably should do some verification here
            let intro_res = recv_command(&mut stream, true);

            peers.write().unwrap().push((
                Mutex::new(stream.try_clone().unwrap()), 
                Mutex::new(stream),
            ));
            peer_ads.write().unwrap().insert(peer_addr);
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
            }
        });
   }


   //Thread to run the main event loop / protocol
   //This thread only deals with peers who have been learned 
   thread::spawn(move || {
        loop{
            //println!("{:?}", peers_clone.read().unwrap().iter().len());
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
                                let mut write = write.lock().unwrap();
                                //if verbose { println!("{:?}", peer_ads.read().unwrap());}
                               

                                let ad_ip_data = data;
                                //Format 'peer_ads' into bytes
                                let mut data = vec![];
                                for peer in peer_ads.read().unwrap().iter() {
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
                                if verbose {println!("Responded with peer list");}

                                let port_number : u16 = ((ad_ip_data[4] as u16) << 8) | ad_ip_data[5] as u16;
                                let advertised_addr = SocketAddr::from(([ad_ip_data[0],
                                        ad_ip_data[1], ad_ip_data[2], ad_ip_data[3]], port_number));

                                peer_ads.write().unwrap().insert(advertised_addr);
                                if verbose {println!("Added {:?} to the peer_list", advertised_addr);}
                            },
                            AD => {
                                let ad_ip_data = data;
                                let port_number : u16 = ((ad_ip_data[4] as u16) << 8) | ad_ip_data[5] as u16;
                                let advertised_addr = SocketAddr::from(([ad_ip_data[0],
                                        ad_ip_data[1], ad_ip_data[2], ad_ip_data[3]], port_number));

                                peer_ads.write().unwrap().insert(advertised_addr);
                                if verbose {println!("Added {:?} to the peer_list", advertised_addr);}
                            },
                            MSG => {
                                println!("MSG: {}", String::from_utf8(data.to_vec()).unwrap());
                            },
                            PING => {
                                if verbose {println!("Received a PING");}
                                send_command(PONG, 0, &vec![], &mut write.lock().unwrap()); 
                                if verbose {println!("Sent a PONG");}
                            }
                            PONG => {
                                if verbose {println!("Receivied a PONG");}
                            },
                            _ => {
                                println!("??? Unknown OpCode ???: ({:?}, Length: {:?})", data, len);
                            }
                        }



                        //println!("{:?}", data);
                    },
                    Err(_) => {}
                }
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
