use crate::start_node;
use crate::threads::RunOp;
use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, SystemTime};

extern crate argparse;

#[test]
fn test_test_one() {
    thread::spawn(|| loop {});
}

#[test]
fn test_test_two() {
    thread::spawn(|| loop {});
}

//TODO: becuase the tests use BTreeSet to sort the elements errors involving duplicate peer
//adertised ips may go through
//Basic network tests
#[test]
fn test_2_nodes() {
    let ((n1_net, n1_accept, n1_ipc), (n1_peers, n1_peer_list, n1_pings), (n1_in, n1_out)) =
        start_node("", "127.0.0.1:7070", false).unwrap();

    let ((n2_net, n2_accept, n2_ipc), (n2_peers, n2_peer_list, n2_pings), (n2_in, n2_out)) =
        start_node("127.0.0.1:7070", "127.0.0.1:9090", false).unwrap();

    //The first things the nodes should output is each other's join command
    let out = n1_out.recv().unwrap();
    assert_eq!(out, RunOp::OnJoin("127.0.0.1:9090".parse().unwrap()));

    let out2 = n2_out.recv().unwrap();
    assert_eq!(out2, RunOp::OnJoin("127.0.0.1:7070".parse().unwrap()));

    n2_net.stop().join();
    n1_net.stop().join();

    n2_ipc.stop().join();
    n1_ipc.stop().join();

    n1_accept.stop().join();
    n2_accept.stop().join();
}

#[test]
fn test_3_nodes() {
    println!("Starting test 3 nodes");
    let ((n1_net, n1_accept, n1_ipc), (n1_peers, n1_peer_list, n1_pings), (n1_in, n1_out)) =
        start_node("", "127.0.0.1:7070", false).unwrap();

    let ((n2_net, n2_accept, n2_ipc), (n2_peers, n2_peer_list, n2_pings), (n2_in, n2_out)) =
        start_node("127.0.0.1:7070", "127.0.0.1:9090", false).unwrap();

    let ((n3_net, n3_accept, n3_ipc), (n3_peers, n3_peer_list, n3_pings), (n3_in, n3_out)) =
        start_node("127.0.0.1:7070", "127.0.0.1:4242", false).unwrap();

    //The first things the nodes should output is each other's join command
    assert_eq!(
        n1_out.recv().unwrap(),
        RunOp::OnJoin("127.0.0.1:9090".parse().unwrap())
    );

    assert_eq!(
        n2_out.recv().unwrap(),
        RunOp::OnJoin("127.0.0.1:7070".parse().unwrap())
    );
    assert_eq!(
        n2_out.recv().unwrap(),
        RunOp::OnJoin("127.0.0.1:4242".parse().unwrap())
    );

    assert_eq!(
        n3_out.recv().unwrap(),
        RunOp::OnJoin("127.0.0.1:7070".parse().unwrap())
    );
    assert_eq!(
        n3_out.recv().unwrap(),
        RunOp::OnJoin("127.0.0.1:9090".parse().unwrap())
    );

    let n1_addrs: BTreeSet<String> = n1_peer_list
        .read()
        .unwrap()
        .iter()
        .map(|(x, y)| y.to_string())
        .collect();
    let n2_addrs: BTreeSet<String> = n2_peer_list
        .read()
        .unwrap()
        .iter()
        .map(|(x, y)| y.to_string())
        .collect();

    let n3_addrs: BTreeSet<String> = n3_peer_list
        .read()
        .unwrap()
        .iter()
        .map(|(x, y)| y.to_string())
        .collect();

    /*println!("{:?} , {:?}", n1_addrs, n1_peers.read().unwrap().len());
    println!("{:?} , {:?}", n2_addrs, n2_peers.read().unwrap().len());
    println!("{:?} , {:?}", n3_addrs, n3_peers.read().unwrap().len());*/

    n2_net.stop().join();
    n1_net.stop().join();

    n3_net.stop().join();
    n2_ipc.stop().join();
    n1_ipc.stop().join();
    n3_ipc.stop().join();
    n1_accept.stop().join();
    n2_accept.stop().join();
    n3_accept.stop().join();
}

//Test the stability of the nodes by adding and removing
#[test]
fn test_4_nodes_stability() {
    let ((n1_net, n1_accept, n1_ipc), (n1_peers, n1_peer_list, n1_pings), (n1_in, n1_out)) =
        start_node("", "127.0.0.1:7070", false).unwrap();

    let ((n2_net, n2_accept, n2_ipc), (n2_peers, n2_peer_list, n2_pings), (n2_in, n2_out)) =
        start_node("127.0.0.1:7070", "127.0.0.1:9090", false).unwrap();

    let ((n3_net, n3_accept, n3_ipc), (n3_peers, n3_peer_list, n3_pings), (n3_in, n3_out)) =
        start_node("127.0.0.1:9090", "127.0.0.1:4242", false).unwrap();

    let ((n4_net, n4_accept, n4_ipc), (n4_peers, n4_peer_list, n4_pings), (n4_in, n4_out)) =
        start_node("127.0.0.1:4242", "127.0.0.1:5555", false).unwrap();

    let mut n1_connection_count = 0;
    let mut n2_connection_count = 0;
    let mut n3_connection_count = 0;
    let mut n4_connection_count = 0;
    while n1_connection_count < 3 {
        match n1_out.recv() {
            Ok(RunOp::OnJoin(_)) => {
                n1_connection_count += 1;
            }
            _ => {}
        }
    }

    let n1_addrs: BTreeSet<String> = n1_peer_list
        .read()
        .unwrap()
        .iter()
        .map(|(x, y)| y.to_string())
        .collect();

    while n2_connection_count < 3 {
        match n2_out.recv() {
            Ok(RunOp::OnJoin(_)) => {
                n2_connection_count += 1;
            }
            _ => {}
        }
    }
    let n2_addrs: BTreeSet<String> = n2_peer_list
        .read()
        .unwrap()
        .iter()
        .map(|(x, y)| y.to_string())
        .collect();

    while n3_connection_count < 3 {
        match n3_out.recv() {
            Ok(RunOp::OnJoin(_)) => {
                n3_connection_count += 1;
            }
            _ => {}
        }
    }

    let n3_addrs: BTreeSet<String> = n3_peer_list
        .read()
        .unwrap()
        .iter()
        .map(|(x, y)| y.to_string())
        .collect();

    while n4_connection_count < 3 {
        match n4_out.recv() {
            Ok(RunOp::OnJoin(_)) => {
                n4_connection_count += 1;
            }
            _ => {}
        }
    }

    let n4_addrs: BTreeSet<String> = n4_peer_list
        .read()
        .unwrap()
        .iter()
        .map(|(x, y)| y.to_string())
        .collect();

    assert_eq!(
        n1_addrs.iter().collect::<Vec<&String>>(),
        vec!["127.0.0.1:4242", "127.0.0.1:5555", "127.0.0.1:9090"]
    );

    assert_eq!(
        n2_addrs.iter().collect::<Vec<&String>>(),
        vec!["127.0.0.1:4242", "127.0.0.1:5555", "127.0.0.1:7070"]
    );
    assert_eq!(
        n3_addrs.iter().collect::<Vec<&String>>(),
        vec!["127.0.0.1:5555", "127.0.0.1:7070", "127.0.0.1:9090"]
    );

    assert_eq!(
        n4_addrs.iter().collect::<Vec<&String>>(),
        vec!["127.0.0.1:4242", "127.0.0.1:7070", "127.0.0.1:9090"]
    );

    println!("{:?} , {:?}", n1_addrs, n1_peers.read().unwrap().len());
    println!("{:?} , {:?}", n2_addrs, n2_peers.read().unwrap().len());
    println!("{:?} , {:?}", n3_addrs, n3_peers.read().unwrap().len());
    println!("{:?} , {:?}", n4_addrs, n4_peers.read().unwrap().len());

    n1_net.stop().join();
    n2_net.stop().join();
    n4_net.stop().join();
    n3_net.stop().join();
    n1_ipc.stop().join();
    n2_ipc.stop().join();
    n3_ipc.stop().join();
    n4_ipc.stop().join();
    n1_accept.stop().join();
    n2_accept.stop().join();
    n3_accept.stop().join();
    n4_accept.stop().join();
}

#[test]
fn test_5_nodes() {
    let ((n1_net, n1_accept, n1_ipc), (n1_peers, n1_peer_list, n1_pings), (n1_in, n1_out)) =
        start_node("", "127.0.0.1:7070", false).unwrap();

    let ((n2_net, n2_accept, n2_ipc), (n2_peers, n2_peer_list, n2_pings), (n2_in, n2_out)) =
        start_node("127.0.0.1:7070", "127.0.0.1:9090", false).unwrap();

    let ((n3_net, n3_accept, n3_ipc), (n3_peers, n3_peer_list, n3_pings), (n3_in, n3_out)) =
        start_node("127.0.0.1:9090", "127.0.0.1:4242", false).unwrap();

    let ((n4_net, n4_accept, n4_ipc), (n4_peers, n4_peer_list, n4_pings), (n4_in, n4_out)) =
        start_node("127.0.0.1:4242", "127.0.0.1:5555", false).unwrap();
    let ((n5_net, n5_accept, n5_ipc), (n5_peers, n5_peer_list, n5_pings), (n5_in, n5_out)) =
        start_node("127.0.0.1:4242", "127.0.0.1:6666", false).unwrap();
    let mut n1_connection_count = 0;
    let mut n2_connection_count = 0;
    let mut n3_connection_count = 0;
    let mut n4_connection_count = 0;
    let mut n5_connection_count = 0;

    while n1_connection_count < 3 {
        match n1_out.recv() {
            Ok(RunOp::OnJoin(_)) => {
                n1_connection_count += 1;
            }
            _ => {}
        }
    }

    let n1_addrs: BTreeSet<String> = n1_peer_list
        .read()
        .unwrap()
        .iter()
        .map(|(x, y)| y.to_string())
        .collect();

    while n2_connection_count < 4 {
        match n2_out.recv() {
            Ok(RunOp::OnJoin(_)) => {
                n2_connection_count += 1;
            }
            _ => {}
        }
    }
    let n2_addrs: BTreeSet<String> = n2_peer_list
        .read()
        .unwrap()
        .iter()
        .map(|(x, y)| y.to_string())
        .collect();

    while n3_connection_count < 4 {
        match n3_out.recv() {
            Ok(RunOp::OnJoin(_)) => {
                n3_connection_count += 1;
            }
            _ => {}
        }
    }

    let n3_addrs: BTreeSet<String> = n3_peer_list
        .read()
        .unwrap()
        .iter()
        .map(|(x, y)| y.to_string())
        .collect();

    while n4_connection_count < 4 {
        match n4_out.recv() {
            Ok(RunOp::OnJoin(_)) => {
                n4_connection_count += 1;
            }
            _ => {}
        }
    }

    let n4_addrs: BTreeSet<String> = n4_peer_list
        .read()
        .unwrap()
        .iter()
        .map(|(x, y)| y.to_string())
        .collect();

    while n5_connection_count < 4 {
        match n5_out.recv() {
            Ok(RunOp::OnJoin(_)) => {
                n5_connection_count += 1;
            }
            _ => {}
        }
    }

    let n5_addrs: BTreeSet<String> = n4_peer_list
        .read()
        .unwrap()
        .iter()
        .map(|(x, y)| y.to_string())
        .collect();

    println!("{:?} , {:?}", n1_addrs, n1_peers.read().unwrap().len());
    println!("{:?} , {:?}", n2_addrs, n2_peers.read().unwrap().len());
    println!("{:?} , {:?}", n3_addrs, n3_peers.read().unwrap().len());
    println!("{:?} , {:?}", n4_addrs, n4_peers.read().unwrap().len());

    n1_net.stop().join();
    n2_net.stop().join();
    n4_net.stop().join();
    n5_net.stop().join();
    n3_net.stop().join();
    n1_ipc.stop().join();
    n2_ipc.stop().join();
    n3_ipc.stop().join();
    n4_ipc.stop().join();
    n5_ipc.stop().join();
    n1_accept.stop().join();
    n2_accept.stop().join();
    n3_accept.stop().join();
    n4_accept.stop().join();
    n5_accept.stop().join();
}

//Unit test
#[test]
fn test_ipc_com() {}
#[test]
fn test_native_com() {}

//Stress tests on the network simulating malicious actors
#[test]
fn test_mal_invalid_args() {}
