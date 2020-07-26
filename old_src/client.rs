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

#[derive(new)]
pub struct TakeetoeNode
{
    //pub connections: Vec<&'a TcpStream>,
    pub total_str: String,
    pub local_addr: String,
    pub local_port: u16,
    pub verbose: bool,
}

#[derive(new)]
//Defines how messages should be responded to
pub struct TakeetoeProtocol {
    pub peer_list : Arc<RwLock<Vec<Arc<RwLock<TcpStream>>>>>,
    //Read the bytes into the command buffer
    //Let it be a vector in case we have to do large chuncks of data for simplicty
    //Probably should set a max size though
    #[new(default)]
    pub command_buffer : Vec<u8>
}

//Node handle is just the 'handle()' function
//Allows network protocols to have a sense of state
pub trait NodeProtocol {
    fn handle(&mut self, data : u8) -> Vec<u8>;
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
    pub fn start<P : 'static + NodeProtocol>(&mut self, protocol: fn(Arc<RwLock<Vec<Arc<RwLock<TcpStream>>>>>) -> P) -> Result<()>
    {
        //Start the server
        let mut listener = TcpListener::bind(self.total_str.clone())?;
        let mut peer_list : Arc<RwLock<Vec<Arc<RwLock<TcpStream>>>>> = Arc::new(RwLock::new(vec![]));

        //Accept every incomming TCP connection on the main thread
        for stream in listener.incoming() {
             println!("New connection");
             let tcp_connection : Arc<RwLock<TcpStream>> = Arc::new(RwLock::new(stream.unwrap()));

             //Setup a new reference to tcp_connection and peer_list that can be owned by the move closure
             let tcp_connection_ref = Arc::clone(&tcp_connection);
             let peer_list_ref = Arc::clone(&peer_list);

             //Start a new thread to handle each connection
             thread::spawn(move || {
                //Create a new instance of the network protocol and pass in the shared peer_list reference
                let mut net_proto = protocol(Arc::clone(&peer_list_ref));
                let mut buffer = [0; 1];
                let mut conn = tcp_connection_ref.read().unwrap();

                //Read in each byte until EOF
                while conn.read(&mut buffer).unwrap() != 0 {
                    // Get the output of each read and write it to the connection
                    let output = net_proto.handle(buffer[0]);
                    tcp_connection_ref.write().unwrap().write(&output[..]);
                }
            });
            peer_list.write().unwrap().push(tcp_connection);
            //Start a new thread to handle each TCPStream
        }
        return Ok(());
//             TcpStream::connect(connecting_ip)?;
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

fn handle_stream(mut stream : TcpStream){
    //stream.set_nonblocking(true).expect("set_nonblocking call failed");
    let mut buf = vec![];
    stream.read_to_end(&mut buf);
    //CONNECTIONS.append(stream);
    println!("read");
    println!("buf: {:?}", buf);
    //loop{
    //    println!("l");
    //    match stream.read_to_end(&mut buf){
    //        Ok(b) => {
    //            println!("Read {:?} bytes", b); 
    //            break;
    //        },
    //        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
    //            println!("error");
    //            continue;
    //        },
    //        Err(e) => panic!("encountered IO error: {}", e),
    //    };
    //    println!("{:?}", buf)
    //}
    
}
