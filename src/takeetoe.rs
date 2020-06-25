extern crate argparse;
extern crate igd;
#[macro_use]
extern crate derive_new;

use std::net::{TcpListener, TcpStream, Ipv4Addr, SocketAddrV4};
use std::io::Result;
use std::io::{self, Read, Write};
use std::thread;
use argparse::{ArgumentParser, StoreTrue, Store, StoreOption, StoreFalse};
use std::convert::AsRef;
use std::sync::{Arc, Mutex, RwLock};
use std::sync::mpsc::{Sender, Receiver};
use std::collections::{HashSet, HashMap};

#[derive(new)]
pub struct TakeetoeNode
{
    //pub connections: Vec<&'a TcpStream>,
    pub total_str: String,
    pub local_addr: String,
    pub local_port: u16,
    pub verbose: bool,
    #[new(default)]
    //A vector that contains both the peers and their corresponding mspc channels
    pub tcp_peers : HashMap<String, Sender<Vec<u8>>>,
}

impl NodeProtocol for TakeetoeProtocol {
    fn handle(&mut self, data : u8) -> Vec<u8> {
        print!("{} Peers: {}", data as char, self.peer_list.read().unwrap().len());

        //if data == 0x01

        for peer in self.peer_list.read().unwrap().iter() {
            //OpCode (1-byte)
            //1 (SYNC_PEERS) | # of peers (1-byte) | ip and port (6-bytes)

        //   println!("{:?}", peer);
            let mut conn = peer.write().unwrap();
           conn.write(b"hello");
           conn.write(&vec![data]);
        }
        return vec![];
    }
}


impl TakeetoeNode
    {
    //Start the node and begin listening and responding to connections
    //Register a function that defines and handles the network protcol
    // Protocol reads in every byte and returns the output to the client
    // peer_list is automatically shared and reference counted between threads
    pub fn start(&mut self) -> Result<()>
    {
        //Begin the INTRO phase (getting a peer list from one node)
        //First connect to the origin peer and get the peer_list
        let origin_peer = TcpStream::connect();

        //Connect to each of the peer lists, and populate self.tcp_peers accordingly

        //Start the server
        let mut listener = TcpListener::bind(self.total_str.clone())?;

        //Accept every incomming TCP connection on the main thread
        for stream in listener.incoming() {
            println!("New connection");
            //Stream ownership is passed to the thread
            let mut tcp_connection: TcpStream = stream.unwrap();
            self.handle_connection(tcp_connection)
        }

        return Ok(());
    }

    //Spawn a new thread to handle the connection
    //Stores the 'Sender' end of the channel in self.peer_list
    pub fn handle_connection(&mut self, tcp_connection: TcpStream){
        //Start a new thread to handle each TCPStream
        //Create a channel that'll be hared across multiple threads
        let (send, recv) = mspc::channel();
        
        //Add to the peer list before ownership is lost
        self.tcp_peers.insert(
            tcp_connection.peer_addr().unwrap().to_string(),
            send
        );

        thread::spawn(move || {
            tcp_connection.set_nonblocking(true).except("failed to set tcp_connection as nonblocking");
            let mut byte_buff: [u8; 1] = [0;1];
            let mut total_buff = vec![];

            //Main event loop
            //on each iteration...
            //  read in a byte, see if it matches a command, if so follow the protocol
            //  handle an incomming 'write' messages on the 'recv' end of the channel if exists
            loop {
                //Read in a byte at a time until a command is recognized
                match tcp_connection.read(&byte_buff) {
                    //EOF: 'num_bytes_read' == 0
                    Ok(num_bytes_read) => {total_buff.push(byte_buff[0]);},
                    //io:ErrorKind::WouldBlock: in this case means that no bytes received
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {continue;},
                    Err(e) => panic!()
                }

                println!("{}", total_buff);

                //Then handle all incomming channels
                match recv.try_recv() {
                    Ok(write_data) => tcp_connection.write(&write_data),
                    //try_recv returns an error on no data or disconnected
                    //https://doc.rust-lang.org/std/sync/mpsc/enum.TryRecvError.html
                    Err(e) => continue
                }

            }
        });
    }

    //Broadcast a message to all nodes on the network
    pub fn broadcast(&self, message: Vec<u8>){
       for peer_id, send_chan in self.tcp_peers {
            send_chan.send(message);
       }
    }
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

    
   //let peer_list = vec![];
   //If a connecting_ip is specified then establish a connection and retrieve the peer list
   if connecting_ip != "0.0.0.0:0" {
        let mut stream = TcpStream::connect(connecting_ip)?;


        //Begin asking for the network's peer list
        stream.write(&[1])?;
        //Get the # of peers
        //stream.read();

        // Connect to each of the specified peers as well

   }

    //let handler : TakeetoeProtocol = TakeetoeProtocol::new();
    let mut node = TakeetoeNode::new(binding_ip.clone(), local_addr.to_string(), local_port, true);
    //let eff = handler.as_ref().map(TakeetoeHandler::handle);
    node.start(TakeetoeProtocol::new);

    return Ok(());
}
