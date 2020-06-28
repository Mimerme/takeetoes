extern crate argparse;
extern crate igd;
#[macro_use]
extern crate derive_new;

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

//Given a TcpStream...
//...spawn a new thread to handle reads and writes
//...returns a tuple of a Sender and Receiver to write and read to the stream
fn spawn_connection_thread(mut tcp_connection: TcpStream) -> Result<(Receiver<u8>, Sender<Vec<u8>>)> 
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

        }
    });

    return Ok((
        recv_in,
        send_out
    ));
}


fn main() -> Result<()>{
    let mut connections : Vec<TcpStream> = vec![];
    //Parse the arguments
    let mut binding_ip = "0.0.0.0:0".to_string();
        //"0.0.0.0:0";
    let mut connecting_ip = "0.0.0.0:0".to_string();
    //"0.0.0.0:0";
    let mut verbose = false;
    let mut unpnp = false;

    {
        let mut parser = ArgumentParser::new();
        parser.set_description("Takeetoe Node");


        parser.refer(&mut verbose)
            .add_option(&["-v", "--verbose"], StoreTrue, "Enable verbose logging");
        parser.refer(&mut unpnp)
            .add_option(&["--upnp"], StoreFalse, "Use unpnp to automatically port forward (router must support it)");
        parser.refer(&mut binding_ip)
            .add_option(&["--binding_ip"], Store, "The ip:port to bind to");
        parser.refer(&mut connecting_ip)
            .add_option(&["--connecting_ip"], Store, "The tcp server:port to connect to");
        parser.parse_args_or_exit();
   
   }

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
    //   1 (1b) | 0..252 (1b) | peer_ip : peer_port + ... (0..252b)
    //'new_peer' now connects to each peer, thus adding itself to each node's peer_list 
    //              |   /     |
    //              |  /      |
    //              | /       |
    //              |/        |
    //              |         |    //                        |


   let mut peers : Arc<RwLock<HashMap<SocketAddr, (Mutex<Sender<Vec<u8>>>, Mutex<Receiver<u8>>)>>> = Arc::new(RwLock::new(HashMap::new()));

   //let peer_list = vec![];
   //If a connecting_ip is specified then establish a connection and retrieve the peer list
   if connecting_ip != "0.0.0.0:0" {
        let mut stream = TcpStream::connect(connecting_ip)?;
        let mut network_header : Vec<u8> = vec![0; 2];

        //Begin asking for the network's peer list
        //[opcode (1), data_length (6), ip : port]
        stream.write(&[1, 6])?;
        //Get the OpCode and Data Length
        stream.read(&mut network_header);
        
        //Format is in:
        //ip1 (4 bytes) | port1 (2 bytes) | ip2 ...  
        let mut peer_data : Vec<u8> = vec![0; network_header[1].into()];
        stream.read(&mut peer_data);

        // Connect to each of the specified peers
        for start in (0..network_header[1].into()).step_by(6) {
            //https://stackoverflow.com/questions/50243866/how-do-i-convert-two-u8-primitives-into-a-u16-primitive?noredirect=1&lq=1
            let port_number : u16 = ((peer_data[start + 4] as u16) << 8) | peer_data[start + 5] as u16;
            let peer_addr = SocketAddr::from(([peer_data[start], peer_data[start + 1], peer_data[start + 2], peer_data[start + 3]], port_number));
            let mut stream = TcpStream::connect(peer_addr).unwrap(); 
            let (read, send) = spawn_connection_thread(stream).unwrap();
            peers.write().unwrap().insert(
                peer_addr,
                (Mutex::new(send), Mutex::new(read))
            );
        }
   }


   //Start the server
   let mut listener = TcpListener::bind(binding_ip)?;
   let mut peers_clone = Arc::clone(&peers);

   //Thread to run the main event loop / protocol
   thread::spawn(move || {
        loop{
            for (string_ip, (send, recv)) in peers_clone.read().unwrap().iter() {
                //send.lock().unwrap().send(b"asaaa".to_vec());
                let recv = recv.lock().unwrap();
                match recv.try_recv() {
                    Ok(data) => {
                        //Check for the opcode of the data first
                        match data {
                            0x01 => {
                                if verbose { println!("{} requested the peer list", string_ip);}
                                //TODO: this might be too slow xd, but it's easier to Google this
                                //stuff xd
                                let peers = peers_clone.read().unwrap();
                                let send = send.lock().unwrap();
                                if verbose { println!("{:?}", peers.keys());}

                                for peer in peers.keys() {
                                    let mut ip = match peer.ip() {
                                        IpAddr::V4(ip) => ip.octets().to_vec(),
                                        IpAddr::V6(ip) => panic!("Protocol currently doesn't support ipv6 :("),
                                    };
                                    let port : u16 = peer.port();
                                    ip.push((port >> 8) as u8);
                                    ip.push(port as u8);
                                    send.send(ip);
                                }
//                                //Get the length of the data
//                                let len = recv.recv().unwrap() as usize;
//
//                                for x in 0..len {
//                                    
//                                }
                                println!("Responded with peer list");
                            },
                            _ => {}
                        }



                        println!("{:?}", data);
                    },
                    Err(_) => {}
                }
            }
       }
   });

   //Accept every incomming TCP connection on the main thread
   for stream in listener.incoming() {
       println!("New connection");
       //Stream ownership is passed to the thread
       let mut tcp_connection: TcpStream = stream.unwrap();
       let peer_addr = tcp_connection.peer_addr().unwrap();

       //Add to the peer list
       let (read, send) = spawn_connection_thread(tcp_connection).unwrap();
       peers.write().unwrap().insert(
            peer_addr,
            (Mutex::new(send), Mutex::new(read))
       );
   }

    return Ok(());
}
