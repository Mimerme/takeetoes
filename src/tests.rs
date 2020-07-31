use crate::threads::RunOp;
use crate::Node;
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
    let mut n1 = Node::new();
    n1.start("", "127.0.0.1:7070", false).unwrap();

    let mut n2 = Node::new();
    n2.start("127.0.0.1:7070", "127.0.0.1:9090", false).unwrap();

    let n1_out = n1.output();
    let n2_out = n2.output();

    //The first things the nodes should output is each other's join command
    let out = n1_out.recv().unwrap();
    assert_eq!(out, RunOp::OnJoin("127.0.0.1:9090".parse().unwrap()));

    let out2 = n2_out.recv().unwrap();
    assert_eq!(out2, RunOp::OnJoin("127.0.0.1:7070".parse().unwrap()));
    assert_eq!(n1.get_peers_arc().read().iter().len(), 1);
    assert_eq!(n2.get_peers_arc().read().iter().len(), 1);
    assert_eq!(n1.get_pings_arc().read().iter().len(), 1);
    assert_eq!(n2.get_pings_arc().read().iter().len(), 1);

    n1.stop();
    n2.stop();
}

#[test]
fn test_3_nodes() {
    let mut n1 = Node::new();
    n1.start("", "127.0.0.1:7070", false).unwrap();

    let mut n2 = Node::new();
    n2.start("127.0.0.1:7070", "127.0.0.1:9090", false).unwrap();

    let mut n3 = Node::new();
    n3.start("127.0.0.1:7070", "127.0.0.1:4242", false).unwrap();

    let n1_out = n1.output();
    let n2_out = n2.output();
    let n3_out = n3.output();

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

    let n1_addrs: BTreeSet<String> = n1
        .get_peer_list_arc()
        .read()
        .unwrap()
        .iter()
        .map(|(x, y)| y.to_string())
        .collect();
    let n2_addrs: BTreeSet<String> = n2
        .get_peer_list_arc()
        .read()
        .unwrap()
        .iter()
        .map(|(x, y)| y.to_string())
        .collect();

    let n3_addrs: BTreeSet<String> = n3
        .get_peer_list_arc()
        .read()
        .unwrap()
        .iter()
        .map(|(x, y)| y.to_string())
        .collect();

    println!(
        "{:?} , {:?}",
        n1_addrs,
        n1.get_peers_arc().read().unwrap().len()
    );
    println!(
        "{:?} , {:?}",
        n2_addrs,
        n2.get_peers_arc().read().unwrap().len()
    );
    println!(
        "{:?} , {:?}",
        n3_addrs,
        n3.get_peers_arc().read().unwrap().len()
    );

    n1.stop();
    n2.stop();
    n3.stop();
}

//Test the stability of the nodes by adding and removing
#[test]
fn test_4_nodes_stability() {
    let mut n1 = Node::new();
    n1.start("", "127.0.0.1:7070", false).unwrap();

    let mut n2 = Node::new();
    n2.start("127.0.0.1:7070", "127.0.0.1:9090", false).unwrap();

    let mut n3 = Node::new();
    n3.start("127.0.0.1:9090", "127.0.0.1:4242", false).unwrap();

    let mut n4 = Node::new();
    n4.start("127.0.0.1:4242", "127.0.0.1:5555", false).unwrap();

    let n1_out = n1.output();
    let n2_out = n2.output();
    let n3_out = n3.output();
    let n4_out = n4.output();

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

    let n1_addrs: BTreeSet<String> = n1
        .get_peer_list_arc()
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
    let n2_addrs: BTreeSet<String> = n2
        .get_peer_list_arc()
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

    let n3_addrs: BTreeSet<String> = n3
        .get_peer_list_arc()
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

    let n4_addrs: BTreeSet<String> = n4
        .get_peer_list_arc()
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

    println!(
        "{:?} , {:?}",
        n1_addrs,
        n1.get_peers_arc().read().unwrap().len()
    );
    println!(
        "{:?} , {:?}",
        n2_addrs,
        n2.get_peers_arc().read().unwrap().len()
    );
    println!(
        "{:?} , {:?}",
        n3_addrs,
        n3.get_peers_arc().read().unwrap().len()
    );
    println!(
        "{:?} , {:?}",
        n4_addrs,
        n4.get_peers_arc().read().unwrap().len()
    );

    n3.stop();
    n1.stop();
    n2.stop();
    n4.stop();
}

#[test]
fn test_5_nodes() {
    let mut n1 = Node::new();
    n1.start("", "127.0.0.1:7070", false).unwrap();

    let mut n2 = Node::new();
    n2.start("127.0.0.1:7070", "127.0.0.1:9090", false).unwrap();

    let mut n3 = Node::new();
    n3.start("127.0.0.1:9090", "127.0.0.1:4242", false).unwrap();

    let mut n4 = Node::new();
    n4.start("127.0.0.1:4242", "127.0.0.1:5555", false).unwrap();

    let mut n5 = Node::new();
    n5.start("127.0.0.1:4242", "127.0.0.1:6666", false).unwrap();

    let n1_out = n1.output();
    let n2_out = n2.output();
    let n3_out = n3.output();
    let n4_out = n4.output();
    let n5_out = n5.output();

    let mut n1_connection_count = 0;
    let mut n2_connection_count = 0;
    let mut n3_connection_count = 0;
    let mut n4_connection_count = 0;
    let mut n5_connection_count = 0;

    while n1_connection_count < 4 {
        match n1_out.recv() {
            Ok(RunOp::OnJoin(_)) => {
                n1_connection_count += 1;
            }
            _ => {}
        }
    }

    let n1_addrs: BTreeSet<String> = n1
        .get_peer_list_arc()
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
    let n2_addrs: BTreeSet<String> = n2
        .get_peer_list_arc()
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

    let n3_addrs: BTreeSet<String> = n3
        .get_peer_list_arc()
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

    let n4_addrs: BTreeSet<String> = n4
        .get_peer_list_arc()
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

    let n5_addrs: BTreeSet<String> = n5
        .get_peer_list_arc()
        .read()
        .unwrap()
        .iter()
        .map(|(x, y)| y.to_string())
        .collect();

    println!(
        "{:?} , {:?}",
        n1_addrs,
        n1.get_peers_arc().read().unwrap().len()
    );
    println!(
        "{:?} , {:?}",
        n2_addrs,
        n2.get_peers_arc().read().unwrap().len()
    );
    println!(
        "{:?} , {:?}",
        n3_addrs,
        n3.get_peers_arc().read().unwrap().len()
    );
    println!(
        "{:?} , {:?}",
        n4_addrs,
        n4.get_peers_arc().read().unwrap().len()
    );
    println!(
        "{:?} , {:?}",
        n5_addrs,
        n5.get_peers_arc().read().unwrap().len()
    );

    n1.stop();
    n2.stop();
    n3.stop();
    n4.stop();
    n5.stop();
}

//Unit test
#[test]
fn test_ipc_com() {}
#[test]
fn test_native_com() {}

//Stress tests on the network simulating malicious actors
#[test]
fn test_mal_invalid_args() {}
