extern crate argparse;
extern crate igd;
#[macro_use]
extern crate derive_new;

use std::time::Duration;
use std::net::{TcpListener, TcpStream, Ipv4Addr, SocketAddrV4, SocketAddr, IpAddr};
use std::io::Result;
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
fn spawn_connection_thread(mut tcp_connection: TcpStream, sleep_dur : Duration) -> Result<(Receiver<u8>, Sender<Vec<u8>>)> 
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

   println!("Binding Addr: {}", &binding_ip);
   println!("Connecting Addr: {}", &connecting_ip);
   let splits : Vec<&str> = binding_ip.split(':').collect();
   let local_addr = splits[0];
   let local_port = splits[1];
   let local_port = local_port.parse::<u16>().unwrap();


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



   let mut peers : Arc<RwLock<HashMap<SocketAddr, (Mutex<Sender<Vec<u8>>>, Mutex<Receiver<u8>>)>>> = Arc::new(RwLock::new(HashMap::new()));
   let mut peer_ads : Arc<RwLock<HashSet<SocketAddr>>> = Arc::new(RwLock::new(HashSet::new()));

   //let peer_list = vec![];
   //If a connecting_ip is specified then establish a connection and retrieve the peer list
   if connecting_ip != "0.0.0.0:0" {
        let c_ip_clone = connecting_ip.clone();
        let mut stream = TcpStream::connect(connecting_ip)?;
        let mut network_header : Vec<u8> = vec![0; 2];

        //Converting the argument into bytes
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

        //If 'punch' opcode = 0x11
        if punch {
            stream.write(&[0x10 | INTRO, 6])?;
        }
        //Otherwise opcode = 0x01
        else{
            stream.write(&[INTRO, 6])?;
        }
        stream.write(&ip)?;
        //Get the OpCode and Data Length
        stream.read(&mut network_header);
        println!("Data Len: {:?}", network_header[1]);


        //Format is in:
        //ip1 (4 bytes) | port1 (2 bytes) | ip2 ...  
        let mut peer_data : Vec<u8> = vec![0; network_header[1].into()];
        println!("{:?}", network_header[0] == INTRO);
        //Read in the network peers
        if network_header[1] != 0 {
            stream.read_exact(&mut peer_data);
        }

        if ((network_header[0] != INTRO_RES) && (network_header[0] != (INTRO_RES | 0x10))){
            println!("{:?}", network_header);
            panic!("INTRO failed. Protocol out of order :(");
        }


        //Add this initial connection to the peer list
        let peer_addr = stream.peer_addr().unwrap();
        let (read, send) = spawn_connection_thread(stream,conn_loop_delay).unwrap();
        peers.write().unwrap().insert(
            peer_addr,
            (Mutex::new(send), Mutex::new(read)),
        );
        peer_ads.write().unwrap().insert(SocketAddr::from_str(&c_ip_clone).unwrap());
 
        println!("Starting peer connections...");
        // Connect to each of the specified peers
        for start in (0..network_header[1].into()).step_by(6) {
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
            //Write the INTRO bytes
            if punch {
                stream.write(&[0x10 | INTRO, 6]);
            }
            else {
                stream.write(&[INTRO, 6]);
            }
            stream.write(&ip);
            //Clear out the response. Lol, probably should do some verification here
            let mut b_buf = vec![0;1];
            stream.read_exact(&mut b_buf);
            stream.read_exact(&mut b_buf);
            stream.read_exact(&mut vec![0; b_buf[0].into()]);

            let (read, send) = spawn_connection_thread(stream, conn_loop_delay).unwrap();
            peers.write().unwrap().insert(
                peer_addr,
                (Mutex::new(send), Mutex::new(read)),
            );
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
                    for (ip,(send, recv)) in peers_debug.write().unwrap().iter() {
                        let send = send.lock().unwrap();
                        let data = (&splits[1].as_bytes()).to_vec();
                        send.send(vec![MSG, data.len() as u8]);
                        send.send(data);
                    }
                }
            }
        });
   }


   //Thread to run the main event loop / protocol
   //This thread only deals with peers who have been learned 
   thread::spawn(move || {
        loop{
            for (string_ip, (send_mut, recv_mut)) in peers_clone.read().unwrap().iter() {
                //send.lock().unwrap().send(b"asaaa".to_vec());
                let recv = recv_mut.lock().unwrap();
                match recv.try_recv() {
                    Ok(data) => {
                        //Check for the opcode of the data first
                        match data {
                            INTRO => {
                                if verbose { println!("{} requested the peer list", string_ip);}
                                //TODO: this might be too slow xd, but it's easier to Google this
                                //stuff xd
                                let peers = peers_clone.read().unwrap();
                                let send = send_mut.lock().unwrap();
                                //if verbose { println!("{:?}", peer_ads.read().unwrap());}
                                
                                send.send(vec![INTRO_RES, (peer_ads.read().unwrap().len() * 6) as u8]);

                                for peer in peer_ads.read().unwrap().iter() {
                                    let mut ip = match peer.ip() {
                                        IpAddr::V4(ip) => ip.octets().to_vec(),
                                        IpAddr::V6(ip) => panic!("Protocol currently doesn't support ipv6 :("),
                                    };
                                    let port : u16 = peer.port();
                                    ip.push((port >> 8) as u8);
                                    ip.push(port as u8);
                                    send.send(ip);
                                }
                                println!("Responded with peer list");

                                //Ignore the data length
                                recv.recv();

                                let advertised_ip_buffer = vec![
                                    recv.recv().unwrap(),
                                    recv.recv().unwrap(),
                                    recv.recv().unwrap(),
                                    recv.recv().unwrap(),
                                    recv.recv().unwrap(),
                                    recv.recv().unwrap(),
                                ];
                                let port_number : u16 = ((advertised_ip_buffer[4] as u16) << 8) | advertised_ip_buffer[5] as u16;
                                let advertised_addr = SocketAddr::from(([advertised_ip_buffer[0], advertised_ip_buffer[1], advertised_ip_buffer[2], advertised_ip_buffer[3]], port_number));

                                peer_ads.write().unwrap().insert(advertised_addr);
                            }
                            MSG => {
                                let data_length : u8 = recv.recv().unwrap();
                                let mut message : Vec<u8> = vec![];
                                for _ in (0..data_length.into()){
                                    message.push(recv.recv().unwrap());
                                }

                                println!("MSG: {}", String::from_utf8(message).unwrap());
                            }
                            ,
                            _ => {
                                println!("Unknown Data???: {:?}", data);
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
       let (read, send) = spawn_connection_thread(tcp_connection, conn_loop_delay).unwrap();
       peers.write().unwrap().insert(
            peer_addr,
            (Mutex::new(send), Mutex::new(read)),
       );
   }

    return Ok(());
}
