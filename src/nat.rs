
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



fn main() {
    let mut listener = TcpListener::bind("127.0.0.1:8080").unwrap();

    for stream in listener.incoming() {
       let mut tcp_connection: TcpStream = stream.unwrap();
       println!("New connection: {:?}", tcp_connection.peer_addr());
       //Stream ownership is passed to the thread

   }
}
