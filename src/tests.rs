//NOTE: since the nodes use overlapping ports in the tests you need to run the test with
//--test-threads=1
use crate::node::NodeOut;
use crate::tak_net::{recv_command, NetOp};
use crate::threads::{be_bytes_to_ip, sock_addr_to_bytes, IpcOp, RunOp};
use crate::Node;
use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, SystemTime};

extern crate argparse;

//A helper struct for comparing node state's to their expected value
//Node::get_state() returns this
#[derive(PartialEq, Debug)]
pub struct NodeState {
    pub peer_list: Vec<String>,
    pub peers_count: usize,
    pub pings_count: usize,
}

//helper function to wait for a node to be ready in a test
fn wait_joins(out: &Node, joins: usize) {
    let mut connection_count = 0;
    while connection_count < joins {
        match out.output().recv() {
            Ok(RunOp::OnJoin(x)) => {
                //println!("{:?}", x);
                connection_count += 1;
            }
            _ => {}
        }
    }
}

fn wait_leaves(out: &Node, leaves: usize) {
    let mut connection_count = 0;
    while connection_count < leaves {
        match out.output().recv() {
            Ok(RunOp::OnLeave(x)) => {
                connection_count += 1;
            }
            _ => {}
        }
    }
}

//TODO: becuase the tests use BTreeSet to sort the elements errors involving duplicate peer
//adertised ips may go through
//Basic network tests
#[test]
fn test_2_nodes() {
    let mut n1 = Node::new();
    n1.start("", "127.0.0.1:7070", "0", false).unwrap();

    let mut n2 = Node::new();
    n2.start("127.0.0.1:7070", "127.0.0.1:9090", "0", false)
        .unwrap();

    let n1_out = n1.output();
    let n2_out = n2.output();

    wait_joins(&n1, 1);
    wait_joins(&n2, 1);

    assert_eq!(
        NodeState {
            peer_list: vec!["127.0.0.1:9090".to_string()],
            peers_count: 1,
            pings_count: 1
        },
        n1.get_state()
    );

    assert_eq!(
        NodeState {
            peer_list: vec!["127.0.0.1:7070".to_string()],
            peers_count: 1,
            pings_count: 1
        },
        n2.get_state()
    );

    n1.stop();
    n2.stop();
}

#[test]
fn test_3_nodes() {
    let mut n1 = Node::new();
    n1.start("", "127.0.0.1:7070", "0", false).unwrap();

    let mut n2 = Node::new();
    n2.start("127.0.0.1:7070", "127.0.0.1:9090", "0", false)
        .unwrap();

    let mut n3 = Node::new();
    n3.start("127.0.0.1:7070", "127.0.0.1:4242", "0", false)
        .unwrap();

    wait_joins(&n1, 2);
    wait_joins(&n2, 2);
    wait_joins(&n3, 2);

    assert_eq!(
        NodeState {
            peer_list: vec!["127.0.0.1:4242".to_string(), "127.0.0.1:9090".to_string()],
            peers_count: 2,
            pings_count: 2
        },
        n1.get_state()
    );

    assert_eq!(
        NodeState {
            peer_list: vec!["127.0.0.1:4242".to_string(), "127.0.0.1:7070".to_string()],
            peers_count: 2,
            pings_count: 2
        },
        n2.get_state()
    );

    assert_eq!(
        NodeState {
            peer_list: vec!["127.0.0.1:7070".to_string(), "127.0.0.1:9090".to_string()],
            peers_count: 2,
            pings_count: 2
        },
        n3.get_state()
    );

    n1.stop();
    n2.stop();
    n3.stop();
}

//Test the stability of the nodes by adding and removing
//Peers one by one all disconnect
#[test]
fn test_4_nodes_stability() {
    let mut n1 = Node::new();
    n1.start("", "127.0.0.1:7070", "0", false).unwrap();

    let mut n2 = Node::new();
    n2.start("127.0.0.1:7070", "127.0.0.1:9090", "0", false)
        .unwrap();

    let mut n3 = Node::new();
    n3.start("127.0.0.1:9090", "127.0.0.1:4242", "0", false)
        .unwrap();

    let mut n4 = Node::new();
    n4.start("127.0.0.1:4242", "127.0.0.1:5555", "0", false)
        .unwrap();

    wait_joins(&n1, 3);
    wait_joins(&n2, 3);
    wait_joins(&n3, 3);
    wait_joins(&n4, 3);

    assert_eq!(
        NodeState {
            peer_list: vec![
                "127.0.0.1:4242".to_string(),
                "127.0.0.1:5555".to_string(),
                "127.0.0.1:9090".to_string()
            ],
            peers_count: 3,
            pings_count: 3
        },
        n1.get_state()
    );

    assert_eq!(
        NodeState {
            peer_list: vec![
                "127.0.0.1:4242".to_string(),
                "127.0.0.1:5555".to_string(),
                "127.0.0.1:7070".to_string()
            ],
            peers_count: 3,
            pings_count: 3
        },
        n2.get_state()
    );

    assert_eq!(
        NodeState {
            peer_list: vec![
                "127.0.0.1:5555".to_string(),
                "127.0.0.1:7070".to_string(),
                "127.0.0.1:9090".to_string()
            ],
            peers_count: 3,
            pings_count: 3
        },
        n3.get_state()
    );

    assert_eq!(
        NodeState {
            peer_list: vec![
                "127.0.0.1:4242".to_string(),
                "127.0.0.1:7070".to_string(),
                "127.0.0.1:9090".to_string()
            ],
            peers_count: 3,
            pings_count: 3
        },
        n4.get_state()
    );

    n3.stop();
    wait_leaves(&n1, 1);
    wait_leaves(&n2, 1);
    wait_leaves(&n4, 1);
    thread::sleep(Duration::from_secs(1));
    assert_eq!(
        NodeState {
            peer_list: vec!["127.0.0.1:5555".to_string(), "127.0.0.1:9090".to_string()],
            peers_count: 2,
            pings_count: 2
        },
        n1.get_state()
    );

    assert_eq!(
        NodeState {
            peer_list: vec!["127.0.0.1:5555".to_string(), "127.0.0.1:7070".to_string()],
            peers_count: 2,
            pings_count: 2
        },
        n2.get_state()
    );

    assert_eq!(
        NodeState {
            peer_list: vec!["127.0.0.1:7070".to_string(), "127.0.0.1:9090".to_string()],
            peers_count: 2,
            pings_count: 2
        },
        n4.get_state()
    );
    n1.stop();
    wait_leaves(&n2, 1);
    wait_leaves(&n4, 1);

    assert_eq!(
        NodeState {
            peer_list: vec!["127.0.0.1:5555".to_string()],
            peers_count: 1,
            pings_count: 1
        },
        n2.get_state()
    );

    assert_eq!(
        NodeState {
            peer_list: vec!["127.0.0.1:9090".to_string()],
            peers_count: 1,
            pings_count: 1
        },
        n4.get_state()
    );
    n2.stop();

    wait_leaves(&n4, 1);
    assert_eq!(
        NodeState {
            peer_list: vec![],
            peers_count: 0,
            pings_count: 0
        },
        n4.get_state()
    );
    n4.stop();
}

//2 nodes leave and one rejoins
#[test]
fn test_5_nodes_stability() {
    let mut n1 = Node::new();
    n1.start("", "127.0.0.1:7070", "0", false).unwrap();

    let mut n2 = Node::new();
    n2.start("127.0.0.1:7070", "127.0.0.1:9090", "0", false)
        .unwrap();

    let mut n3 = Node::new();
    n3.start("127.0.0.1:9090", "127.0.0.1:4242", "0", false)
        .unwrap();

    let mut n4 = Node::new();
    n4.start("127.0.0.1:4242", "127.0.0.1:5555", "0", false)
        .unwrap();

    let mut n5 = Node::new();
    n5.start("127.0.0.1:4242", "127.0.0.1:6666", "0", false)
        .unwrap();

    wait_joins(&n1, 4);
    wait_joins(&n2, 4);
    wait_joins(&n3, 4);
    wait_joins(&n4, 4);
    wait_joins(&n5, 4);

    assert_eq!(
        NodeState {
            peer_list: vec![
                "127.0.0.1:4242".to_string(),
                "127.0.0.1:5555".to_string(),
                "127.0.0.1:6666".to_string(),
                "127.0.0.1:9090".to_string()
            ],
            peers_count: 4,
            pings_count: 4
        },
        n1.get_state()
    );

    assert_eq!(
        NodeState {
            peer_list: vec![
                "127.0.0.1:4242".to_string(),
                "127.0.0.1:5555".to_string(),
                "127.0.0.1:6666".to_string(),
                "127.0.0.1:7070".to_string()
            ],
            peers_count: 4,
            pings_count: 4
        },
        n2.get_state()
    );

    assert_eq!(
        NodeState {
            peer_list: vec![
                "127.0.0.1:5555".to_string(),
                "127.0.0.1:6666".to_string(),
                "127.0.0.1:7070".to_string(),
                "127.0.0.1:9090".to_string()
            ],
            peers_count: 4,
            pings_count: 4
        },
        n3.get_state()
    );

    assert_eq!(
        NodeState {
            peer_list: vec![
                "127.0.0.1:4242".to_string(),
                "127.0.0.1:6666".to_string(),
                "127.0.0.1:7070".to_string(),
                "127.0.0.1:9090".to_string()
            ],
            peers_count: 4,
            pings_count: 4
        },
        n4.get_state()
    );

    assert_eq!(
        NodeState {
            peer_list: vec![
                "127.0.0.1:4242".to_string(),
                "127.0.0.1:6666".to_string(),
                "127.0.0.1:7070".to_string(),
                "127.0.0.1:9090".to_string()
            ],
            peers_count: 4,
            pings_count: 4
        },
        n4.get_state()
    );

    assert_eq!(
        NodeState {
            peer_list: vec![
                "127.0.0.1:4242".to_string(),
                "127.0.0.1:5555".to_string(),
                "127.0.0.1:7070".to_string(),
                "127.0.0.1:9090".to_string()
            ],
            peers_count: 4,
            pings_count: 4
        },
        n5.get_state()
    );

    n3.stop();
    n5.stop();

    wait_leaves(&n1, 2);
    wait_leaves(&n4, 2);
    wait_leaves(&n2, 2);

    assert_eq!(
        NodeState {
            peer_list: vec!["127.0.0.1:5555".to_string(), "127.0.0.1:9090".to_string()],
            peers_count: 2,
            pings_count: 2
        },
        n1.get_state()
    );

    assert_eq!(
        NodeState {
            peer_list: vec!["127.0.0.1:5555".to_string(), "127.0.0.1:7070".to_string()],
            peers_count: 2,
            pings_count: 2
        },
        n2.get_state()
    );

    assert_eq!(
        NodeState {
            peer_list: vec!["127.0.0.1:7070".to_string(), "127.0.0.1:9090".to_string()],
            peers_count: 2,
            pings_count: 2
        },
        n4.get_state()
    );

    let mut n5 = Node::new();
    n5.start("127.0.0.1:9090", "127.0.0.1:6969", "0", false)
        .unwrap();
    wait_joins(&n1, 1);
    wait_joins(&n2, 1);
    wait_joins(&n4, 1);
    wait_joins(&n5, 3);

    assert_eq!(
        NodeState {
            peer_list: vec![
                "127.0.0.1:5555".to_string(),
                "127.0.0.1:6969".to_string(),
                "127.0.0.1:9090".to_string()
            ],
            peers_count: 3,
            pings_count: 3
        },
        n1.get_state()
    );

    assert_eq!(
        NodeState {
            peer_list: vec![
                "127.0.0.1:5555".to_string(),
                "127.0.0.1:6969".to_string(),
                "127.0.0.1:7070".to_string()
            ],
            peers_count: 3,
            pings_count: 3
        },
        n2.get_state()
    );

    assert_eq!(
        NodeState {
            peer_list: vec![
                "127.0.0.1:6969".to_string(),
                "127.0.0.1:7070".to_string(),
                "127.0.0.1:9090".to_string()
            ],
            peers_count: 3,
            pings_count: 3
        },
        n4.get_state()
    );

    assert_eq!(
        NodeState {
            peer_list: vec![
                "127.0.0.1:5555".to_string(),
                "127.0.0.1:7070".to_string(),
                "127.0.0.1:9090".to_string()
            ],
            peers_count: 3,
            pings_count: 3
        },
        n5.get_state()
    );

    n1.stop();
    n4.stop();
    n2.stop();
    n5.stop();
}

#[test]
fn test_ipc_join() {
    //Connect to the ipc communications
    let mut n1 = Node::new();
    n1.start("", "127.0.0.1:7070", "4269", false).unwrap();
    let mut n1_ipc = TcpStream::connect("127.0.0.1:4269").unwrap();

    let mut n2 = Node::new();
    n2.start("127.0.0.1:7070", "127.0.0.1:9090", "6942", false)
        .unwrap();
    let mut n2_ipc = TcpStream::connect("127.0.0.1:6942").unwrap();

    let mut n3 = Node::new();
    n3.start("127.0.0.1:7070", "127.0.0.1:4242", "0", false)
        .unwrap();

    //let mut buf = [0 as u8; 8];
    //n1_ipc.read_exact(&mut buf).unwrap();
    //println!("net buf: {:?}", buf);

    //Test the ipc stream contents to make sure that all the join requests were received properly
    let (opcode, datalen, data) = recv_command(&mut n1_ipc, true).unwrap();

    //println!("node 1 started");
    //println!("{:?} {:?} {:?}", opcode, datalen, data);
    assert_eq!(opcode, IpcOp::OnJoin as u8);
    assert_eq!(datalen, 6);
    assert_eq!(
        "127.0.0.1:9090".parse::<SocketAddr>().unwrap(),
        be_bytes_to_ip(&data)
    );

    let (opcode, datalen, data) = recv_command(&mut n1_ipc, true).unwrap();
    assert_eq!(opcode, IpcOp::OnJoin as u8);
    assert_eq!(datalen, 6);
    assert_eq!(
        "127.0.0.1:4242".parse::<SocketAddr>().unwrap(),
        be_bytes_to_ip(&data)
    );

    let (opcode, datalen, data) = recv_command(&mut n2_ipc, true).unwrap();
    assert_eq!(opcode, IpcOp::OnJoin as u8);
    assert_eq!(datalen, 6);
    assert_eq!(
        "127.0.0.1:7070".parse::<SocketAddr>().unwrap(),
        be_bytes_to_ip(&data)
    );

    let (opcode, datalen, data) = recv_command(&mut n2_ipc, true).unwrap();
    assert_eq!(opcode, IpcOp::OnJoin as u8);
    assert_eq!(datalen, 6);
    assert_eq!(
        "127.0.0.1:4242".parse::<SocketAddr>().unwrap(),
        be_bytes_to_ip(&data)
    );

    n1.stop();
    n2.stop();
    n3.stop();
}

#[test]
fn test_ipc_leave() {
    //Connect to the ipc communications
    let mut n1 = Node::new();
    n1.start("", "127.0.0.1:7070", "4269", false).unwrap();
    let mut n1_ipc = TcpStream::connect("127.0.0.1:4269").unwrap();

    let mut n2 = Node::new();
    n2.start("127.0.0.1:7070", "127.0.0.1:9090", "6942", false)
        .unwrap();
    let mut n2_ipc = TcpStream::connect("127.0.0.1:6942").unwrap();

    let mut n3 = Node::new();
    n3.start("127.0.0.1:7070", "127.0.0.1:4242", "0", false)
        .unwrap();

    //Go through the join commands
    let (opcode, datalen, data) = recv_command(&mut n1_ipc, true).unwrap();
    let (opcode, datalen, data) = recv_command(&mut n1_ipc, true).unwrap();
    let (opcode, datalen, data) = recv_command(&mut n2_ipc, true).unwrap();
    let (opcode, datalen, data) = recv_command(&mut n2_ipc, true).unwrap();

    n1.stop();

    let (opcode, datalen, data) = recv_command(&mut n2_ipc, true).unwrap();
    assert_eq!(opcode, IpcOp::OnLeave as u8);
    assert_eq!(datalen, 6);
    assert_eq!(
        "127.0.0.1:7070".parse::<SocketAddr>().unwrap(),
        be_bytes_to_ip(&data)
    );

    n3.stop();
    let (opcode, datalen, data) = recv_command(&mut n2_ipc, true).unwrap();
    assert_eq!(opcode, IpcOp::OnLeave as u8);
    assert_eq!(datalen, 6);
    assert_eq!(
        "127.0.0.1:4242".parse::<SocketAddr>().unwrap(),
        be_bytes_to_ip(&data)
    );

    n2.stop();
}
#[test]
fn test_ipc_ping() {
    //Connect to the ipc communications
    let mut n1 = Node::new();
    n1.start("", "127.0.0.1:7070", "4269", false).unwrap();
    let mut n1_ipc = TcpStream::connect("127.0.0.1:4269").unwrap();

    let mut n2 = Node::new();
    n2.start("127.0.0.1:7070", "127.0.0.1:9090", "6942", false)
        .unwrap();
    let mut n2_ipc = TcpStream::connect("127.0.0.1:6942").unwrap();

    let mut n3 = Node::new();
    n3.start("127.0.0.1:7070", "127.0.0.1:4242", "0", false)
        .unwrap();

    //Go through the join commands
    let (opcode, datalen, data) = recv_command(&mut n1_ipc, true).unwrap();
    let (opcode, datalen, data) = recv_command(&mut n1_ipc, true).unwrap();
    let (opcode, datalen, data) = recv_command(&mut n2_ipc, true).unwrap();
    let (opcode, datalen, data) = recv_command(&mut n2_ipc, true).unwrap();

    //Request for the ping table
    n1_ipc.write(&vec![1, 0]);

    let (opcode, datalen, data) = recv_command(&mut n1_ipc, true).unwrap();
    assert_eq!(opcode, IpcOp::PingRes as u8);
    assert_eq!(datalen, 30);
    assert_eq!(
        "127.0.0.1:9090".parse::<SocketAddr>().unwrap(),
        be_bytes_to_ip(&data[0..6])
    );
    assert_eq!(
        "127.0.0.1:4242".parse::<SocketAddr>().unwrap(),
        be_bytes_to_ip(&data[15..21])
    );

    n1.stop();
    //First wait for the leave command
    let (opcode, datalen, data) = recv_command(&mut n2_ipc, true).unwrap();
    //Then write
    n2_ipc.write(&vec![1, 0]);

    let (opcode, datalen, data) = recv_command(&mut n2_ipc, true).unwrap();
    assert_eq!(opcode, IpcOp::PingRes as u8);
    assert_eq!(datalen, 15);
    assert_eq!(
        "127.0.0.1:4242".parse::<SocketAddr>().unwrap(),
        be_bytes_to_ip(&data[0..6])
    );

    n2.stop();
    n3.stop();
}
#[test]
fn test_ipc_broadcast() {
    //Connect to the ipc communications
    let mut n1 = Node::new();
    n1.start("", "127.0.0.1:7070", "4269", false).unwrap();
    let mut n1_ipc = TcpStream::connect("127.0.0.1:4269").unwrap();

    let mut n2 = Node::new();
    n2.start("127.0.0.1:7070", "127.0.0.1:9090", "6942", false)
        .unwrap();
    let mut n2_ipc = TcpStream::connect("127.0.0.1:6942").unwrap();

    let mut n3 = Node::new();
    n3.start("127.0.0.1:7070", "127.0.0.1:4242", "4444", false)
        .unwrap();
    let mut n3_ipc = TcpStream::connect("127.0.0.1:4444").unwrap();

    //Go through the join commands
    let (opcode, datalen, data) = recv_command(&mut n1_ipc, true).unwrap();
    let (opcode, datalen, data) = recv_command(&mut n1_ipc, true).unwrap();
    let (opcode, datalen, data) = recv_command(&mut n2_ipc, true).unwrap();
    let (opcode, datalen, data) = recv_command(&mut n2_ipc, true).unwrap();
    let (opcode, datalen, data) = recv_command(&mut n3_ipc, true).unwrap();
    let (opcode, datalen, data) = recv_command(&mut n3_ipc, true).unwrap();

    //Send a broadcast
    let mut command = vec![3, 5];
    command.extend(b"obama");
    n1_ipc.write(&command);

    let (opcode, datalen, data) = recv_command(&mut n2_ipc, true).unwrap();
    assert_eq!(opcode, IpcOp::Broadcast as u8);
    assert_eq!(datalen, 11);
    assert_eq!(
        "127.0.0.1:7070".parse::<SocketAddr>().unwrap(),
        be_bytes_to_ip(&data[0..6])
    );
    assert_eq!(b"obama".to_vec(), data[6..11].to_vec());

    let (opcode, datalen, data) = recv_command(&mut n3_ipc, true).unwrap();
    assert_eq!(opcode, IpcOp::Broadcast as u8);
    assert_eq!(datalen, 11);
    assert_eq!(
        "127.0.0.1:7070".parse::<SocketAddr>().unwrap(),
        be_bytes_to_ip(&data[0..6])
    );
    assert_eq!(b"obama".to_vec(), data[6..11].to_vec());

    let mut command = vec![3, 7];
    command.extend(b"blaster");
    n3_ipc.write(&command);

    let (opcode, datalen, data) = recv_command(&mut n2_ipc, true).unwrap();

    assert_eq!(opcode, IpcOp::Broadcast as u8);
    assert_eq!(datalen, 13);
    assert_eq!(
        "127.0.0.1:4242".parse::<SocketAddr>().unwrap(),
        be_bytes_to_ip(&data[0..6])
    );
    assert_eq!(b"blaster".to_vec(), data[6..13].to_vec());

    let (opcode, datalen, data) = recv_command(&mut n1_ipc, true).unwrap();
    assert_eq!(opcode, IpcOp::Broadcast as u8);
    assert_eq!(datalen, 13);
    assert_eq!(
        "127.0.0.1:4242".parse::<SocketAddr>().unwrap(),
        be_bytes_to_ip(&data[0..6])
    );
    assert_eq!(b"blaster".to_vec(), data[6..13].to_vec());

    n1.stop();
    n2.stop();
    n3.stop();
}

//The stability tests actually test the native join and leave
//so make this a seperate unit test or no?
#[test]
fn test_native_join() {
    //Connect to the ipc communications
    let mut n1 = Node::new();
    n1.start("", "127.0.0.1:7070", "0", false).unwrap();

    let mut n2 = Node::new();
    n2.start("127.0.0.1:7070", "127.0.0.1:9090", "0", false)
        .unwrap();

    let mut n3 = Node::new();
    n3.start("127.0.0.1:7070", "127.0.0.1:4242", "0", false)
        .unwrap();

    //let mut buf = [0 as u8; 8];
    //n1_ipc.read_exact(&mut buf).unwrap();
    //println!("net buf: {:?}", buf);

    let n1_out = n1.output();
    let n2_out = n2.output();
    let n3_out = n3.output();

    assert_eq!(
        RunOp::OnJoin("127.0.0.1:9090".parse::<SocketAddr>().unwrap()),
        n1_out.recv().unwrap()
    );
    assert_eq!(
        RunOp::OnJoin("127.0.0.1:4242".parse::<SocketAddr>().unwrap()),
        n1_out.recv().unwrap()
    );

    assert_eq!(
        RunOp::OnJoin("127.0.0.1:7070".parse::<SocketAddr>().unwrap()),
        n2_out.recv().unwrap()
    );
    assert_eq!(
        RunOp::OnJoin("127.0.0.1:4242".parse::<SocketAddr>().unwrap()),
        n2_out.recv().unwrap()
    );

    assert_eq!(
        RunOp::OnJoin("127.0.0.1:7070".parse::<SocketAddr>().unwrap()),
        n3_out.recv().unwrap()
    );
    assert_eq!(
        RunOp::OnJoin("127.0.0.1:9090".parse::<SocketAddr>().unwrap()),
        n3_out.recv().unwrap()
    );

    n1.stop();
    n2.stop();
    n3.stop();
}
#[test]
fn test_native_leave() {
    //Connect to the ipc communications
    let mut n1 = Node::new();
    n1.start("", "127.0.0.1:7070", "0", false).unwrap();

    let mut n2 = Node::new();
    n2.start("127.0.0.1:7070", "127.0.0.1:9090", "0", false)
        .unwrap();

    let mut n3 = Node::new();
    n3.start("127.0.0.1:7070", "127.0.0.1:4242", "0", false)
        .unwrap();

    let n1_out = n1.output();
    let n2_out = n2.output();
    let n3_out = n3.output();

    //Discard the join receives
    n1_out.recv();
    n1_out.recv();

    n2_out.recv();
    n2_out.recv();

    n3_out.recv();
    n3_out.recv();

    n1.stop();

    assert_eq!(
        RunOp::OnLeave("127.0.0.1:7070".parse().unwrap()),
        n2_out.recv().unwrap()
    );
    assert_eq!(
        RunOp::OnLeave("127.0.0.1:7070".parse().unwrap()),
        n3_out.recv().unwrap()
    );

    n3.stop();

    assert_eq!(
        RunOp::OnLeave("127.0.0.1:4242".parse().unwrap()),
        n2_out.recv().unwrap()
    );

    n2.stop();
}
#[test]
fn test_native_ping() {
    //Connect to the ipc communications
    let mut n1 = Node::new();
    n1.start("", "127.0.0.1:7070", "0", false).unwrap();

    let mut n2 = Node::new();
    n2.start("127.0.0.1:7070", "127.0.0.1:9090", "0", false)
        .unwrap();

    let mut n3 = Node::new();
    n3.start("127.0.0.1:7070", "127.0.0.1:4242", "0", false)
        .unwrap();

    let n1_out = n1.output();
    let n2_out = n2.output();
    let n3_out = n3.output();
    let n1_in = n1.input();
    let n2_in = n2.input();
    let n3_in = n3.input();

    //Discard the join receives
    n1_out.recv();
    n1_out.recv();

    n2_out.recv();
    n2_out.recv();

    n3_out.recv();
    n3_out.recv();

    //Request for the ping table
    n1_in.send(RunOp::PingReq);

    if let Ok(RunOp::PingRes(vec)) = n1_out.recv() {
        assert_eq!(vec.len(), 2);
    } else {
        assert!(false);
    }

    n1.stop();
    n2_out.recv();
    n2_in.send(RunOp::PingReq);

    if let Ok(RunOp::PingRes(vec)) = n2_out.recv() {
        assert_eq!(vec.len(), 1);
    } else {
        assert!(false);
    }

    n2.stop();
    n3.stop();
}
#[test]
fn test_native_broadcast() {
    //Connect to the ipc communications
    let mut n1 = Node::new();
    n1.start("", "127.0.0.1:7070", "0", false).unwrap();

    let mut n2 = Node::new();
    n2.start("127.0.0.1:7070", "127.0.0.1:9090", "0", false)
        .unwrap();

    let mut n3 = Node::new();
    n3.start("127.0.0.1:7070", "127.0.0.1:4242", "0", false)
        .unwrap();

    let n1_out = n1.output();
    let n2_out = n2.output();
    let n3_out = n3.output();
    let n1_in = n1.input();
    let n2_in = n2.input();
    let n3_in = n3.input();

    //Discard the join receives
    n1_out.recv();
    n1_out.recv();

    n2_out.recv();
    n2_out.recv();

    n3_out.recv();
    n3_out.recv();

    //Send a broadcast
    n1_in.send(RunOp::BroadcastSend(b"obama".to_vec()));

    assert_eq!(
        RunOp::BroadcastRecv("127.0.0.1:7070".parse().unwrap(), b"obama".to_vec()),
        n3_out.recv().unwrap()
    );
    assert_eq!(
        RunOp::BroadcastRecv("127.0.0.1:7070".parse().unwrap(), b"obama".to_vec()),
        n2_out.recv().unwrap()
    );

    n3_in.send(RunOp::BroadcastSend(b"blaster".to_vec()));
    assert_eq!(
        RunOp::BroadcastRecv("127.0.0.1:4242".parse().unwrap(), b"blaster".to_vec()),
        n2_out.recv().unwrap()
    );
    assert_eq!(
        RunOp::BroadcastRecv("127.0.0.1:4242".parse().unwrap(), b"blaster".to_vec()),
        n1_out.recv().unwrap()
    );

    n1.stop();
    n2.stop();
    n3.stop();
}
