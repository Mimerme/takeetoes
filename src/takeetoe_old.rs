extern crate argparse;
extern crate igd;
#[macro_use]
extern crate derive_new;

use std::net::{TcpListener, TcpStream, Ipv4Addr, SocketAddrV4};
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

#[derive(new)]
pub struct TakeetoeNode
{
    //pub connections: Vec<&'a TcpStream>,
    pub total_str: String,
    pub local_addr: String,
    pub local_port: u16,
    pub verbose: bool,
}

//impl<T> Deref for TakeetoeNode {
//    type Target = TakeetoeNode;
//    fn deref(&self) -> &self::Target {
//        &self
//    }
//}
//
//impl DerefMut for TakeetoeNode {
//    fn deref_mut(&mut self) -> &mut self::Target {
//        &mut self
//    }
//}


impl TakeetoeNode
    {
    //Start the node and begin listening and responding to connections
    //Register a function that defines and handles the network protcol
    // Protocol reads in every byte and returns the output to the client
    // peer_list is automatically shared and reference counted between threads
    pub fn start<F>(&self, mut handler : F, connect_peer : &str) -> Result<()>
        where F: FnMut(String, Sender<Vec<u8>>) -> (),
    {
        //Begin the INTRO phase (getting a peer list from one node)
        //First connect to the origin peer and get the peer_list
        let mut origin_peer = TcpStream::connect(connect_peer);
        

        //Connect to each of the peer lists, and populate self.tcp_peers accordingly

        //Start the server
        let mut listener = TcpListener::bind(self.total_str.clone())?;

            //Accept every incomming TCP connection on the main thread
            for stream in listener.incoming() {
                println!("New connection");
                //Stream ownership is passed to the thread
                let mut tcp_connection: TcpStream = stream.unwrap();
                let (addr, send) = self.handle_connection(tcp_connection).unwrap();
                handler(addr, send);
            }

        return Ok(());
    }

    //Spawn a new thread to handle the connection
    //Stores the 'Sender' end of the channel in self.peer_list
    pub fn handle_connection<M>(&self, mut tcp_connection: TcpStream, mut message_handler : M) -> Result<(String, Sender<Vec<u8>>)> 
        where M: FnMut(Vec<u8>) -> Vec<u8> + Sync + Send,
    {
        //Start a new thread to handle each TCPStream
        //Create a channel that'll be hared across multiple threads
        let (send, recv) = mpsc::channel::<Vec<u8>>();
        let conn_addr = tcp_connection.peer_addr().unwrap().to_string();   
    
        thread::spawn(move || {
            tcp_connection.set_nonblocking(true).expect("failed to set tcp_connection as nonblocking");
            let mut byte_buff: Vec<u8> = vec![0; 1];
            let mut total_buff = vec![];

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
                            total_buff.push(byte_buff[0]);
                            total_buff = message_handler(total_buff);
                        }
                    },
                    //io:ErrorKind::WouldBlock: in this case means that no bytes received
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {},
                    Err(e) => panic!()
                }

                //Then handle all incomming channels and write it to the TCPStream
                match recv.try_recv() {
                    Ok(write_data) => {tcp_connection.write(&write_data);},
                    //try_recv returns an error on no data or disconnected
                    //https://doc.rust-lang.org/std/sync/mpsc/enum.TryRecvError.html
                    Err(e) => {}
                }

            }
        });

        return Ok((
            conn_addr,
            send
        ));

    }

    //Broadcast a message to all nodes on the network
    //pub fn broadcast(&self, message: Vec<u8>){
    //    println!("broadcasting");
    //   for (peer_id, send_chan) in self.tcp_peers.iter() {
    //        println!("broadcast: {:?}", peer_id);
    //        send_chan.lock().unwrap().send(message.clone());
    //   }
    //}
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
    
    //Share a list of the Sender(s) to each connection's event loop
    let mut writters : Arc<RwLock<Vec<Mutex<Sender<Vec<u8>>>>>> = Arc::new(RwLock::new(vec![]));
    let writters1 = Arc::clone(&writters);
    let writters2 = Arc::clone(&writters);

    //Start the file watching thread...
    thread::spawn(move ||{
        let writters = writters1;
        loop {
            thread::sleep(time::Duration::from_millis(1000));
            for conn in writters.read().unwrap().iter(){
                println!("Writting...");
                conn.lock().unwrap().send(b"OBAMA".to_vec());
            }
        }
    });


    node.start(move |conn_addr, writter|{
        let writters = &writters2;
        //Save the writter to the list
        writters.write().unwrap().push(Mutex::new(writter));
    });

    //NOTE: Kinda some monkey behavior here so let me explain
    //Arc needs to wrap either a Mutex or a RwLock....
    //...opt for RwLock because a Mutex would just result in a deadlock
    //But TakeetoeNode doesn't actually mutate anyting besides in the start() method...
    //...but RwLock only returns mutable references with write(), and immutable references with read()
    //So calling broadcast()/anything that uses the channel will need to be done with read()
    //and start() needs to be called through write()
    //let mut xd = Arc::clone(&yeet);
    //let t = thread::spawn(move || { yeet.get_mut().unwrap().start();});
    //thread::sleep(time::Duration::from_millis(5000));
    ///println!("BROAD");
    //xd.read().unwrap().broadcast(b"Hello".to_vec());

    //t.join();
    return Ok(());
}
