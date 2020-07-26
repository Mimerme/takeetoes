use argparse::{ArgumentParser, Store, StoreFalse, StoreOption, StoreTrue};
use std::boxed::Box;
use std::collections::{HashMap, HashSet};
use std::io::Result;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::ops::{Deref, DerefMut};
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::{thread, time};

#[test]
fn test_obama() {
    println!("test");
    let a = usize::from_be_bytes([0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x01]);
    println!("{}", a);
}

fn main() {
    let mut listener = TcpListener::bind("127.0.0.1:8080").unwrap();
    listener.accept();

    for stream in listener.incoming() {
        let mut tcp_connection: TcpStream = stream.unwrap();
        println!("New connection: {:?}", tcp_connection.peer_addr());
        //Stream ownership is passed to the thread
    }
}
