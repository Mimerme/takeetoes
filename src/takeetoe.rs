#![allow(warnings)]

extern crate argparse;
use crate::threads::RunOp;
use argparse::{ArgumentParser, Store, StoreFalse, StoreOption, StoreTrue};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Result;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};
use std::{thread, time};
use stoppable_thread::StoppableHandle;

//Import some functions in the other files
pub mod node;
pub mod tak_net;
pub mod tests;
pub mod threads;
use log::{debug, info};
use log::{Level, Metadata, Record};
use log::{LevelFilter, SetLoggerError};
use node::Node;
use threads::{start_accept_thread, start_network_thread, Command};

struct SimpleLogger;
static LOGGER: SimpleLogger = SimpleLogger;
impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        false
    }

    fn log(&self, record: &Record) {
        let since_epoch = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        println!(
            "[{:?} | {}]    {}",
            since_epoch,
            record.level(),
            record.args(),
        );
    }

    fn flush(&self) {}
}

pub fn init_logger() -> std::result::Result<(), SetLoggerError> {
    log::set_logger(&LOGGER).map(|()| log::set_max_level(LevelFilter::Debug))
}

/*
 * JUST A HEADS UP
 * ===============
 * 'Peers' is a maping of the host-to-peer-addr to IN and OUT streams to a peer
 * 'PeerList' is a maping of the host-to-peer-addr to its net_address
 * They are very similar, and although they are often modified together, they are technically
 * independent
 * This is because Peers can connect to the network before they advertise their address
 * TODO: make this official. call them 'leechers'
 */
pub type Peers = Arc<RwLock<HashMap<SocketAddr, (Mutex<TcpStream>, Mutex<TcpStream>)>>>;
pub type PeerList = Arc<RwLock<HashMap<SocketAddr, SocketAddr>>>;
pub type Pings = Arc<RwLock<HashMap<SocketAddr, (u8, SystemTime)>>>;

//use net::{recv_command, send_command};

fn main() -> Result<()> {
    /*
     * Begin parsing the arguments here
     * available arguments...
     * --unpnp, -d, -t, -net_debug, --punch, --binding_ip, --connecting_ip, --punch_ip,
     * --delay, -p
     *
     */

    //IP to bind/advertise
    let mut binding_ip = "0.0.0.0:0".to_string();
    //IP of intro node
    let mut connecting_ip = "0.0.0.0:0".to_string();
    //IP of punch server
    let mut punch_ip = "0.0.0.0:0".to_string();

    //Delay each the main event loop
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
        parser.refer(&mut delay).add_option(
            &["--delay"],
            Store,
            "Sets the delay of the main event loop",
        );

        parser.refer(&mut project_dir).add_option(
            &["-p", "--project_dir"],
            Store,
            "The project directory to syncronize",
        );

        parser.parse_args_or_exit();
    }

    init_logger();

    //In the case that the node is run as a binary we can discard all of the channel handles and
    //extra data structures
    let mut main = Node::new();
    main.start(&connecting_ip, &binding_ip, debug);
    main.wait();

    return Ok(());
}
