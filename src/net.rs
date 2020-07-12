use std::io::{Error, ErrorKind};
use std::net::{TcpListener, TcpStream, Ipv4Addr, SocketAddrV4, SocketAddr, IpAddr};

fn recv_command(connection : &mut TcpStream, block : bool)->Result<(u8, u64, Vec<u8>)>{
    let net_debug = true;
    let mut network_header = vec[0;9];

    connection.set_nonblocking(!block).expect("set_nonblocking failed");
    // Keep peeking until we have a network header in the connection stream
    //println!("recv_command()");

    //println!("Loop: {:?}", block);
    //Block until an entire command is received
    if block {
        //println!("Blocking...");
        connection.read_exact(&mut network_header);
        let data_len = u64::from_be_bytes(&network_header[1..]);
        let dl_index : usize =  data_len.into();
        let mut command = vec![0; dl_index];

        //println!("Waiting for data... {:?}", dl_index);
        if data_len != 0 { 
            connection.read_exact(&mut command);
            if net_debug { println!("Received command (blocking): {:?} | {:?} | {:?}", network_header[0], data_len, command);}
            return Ok((network_header[0], data_len, command));
        }
        else {
            if net_debug { println!("Received command (blocking): {:?} | {:?}", newtork_header[0], data_len);}
            return Ok((newtork_header[0], 0, vec![]));
        }
    }
    else {

        let ret = connection.peek(&mut network_header)?;

        if ret == 0 {
            return Err(Error::new(ErrorKind::UnexpectedEof, "EOF"));
        }
        else if ret < network_header.len() {
            return Err(Error::new(ErrorKind::WouldBlock, "would block"));
        }
        let data_len = u64::from_be_bytes(&network_header[1..]);
        let dl_index : usize =  data_len.into();
        
        let mut command = vec![0; dl_index + 1 + 8];
        //println!("NO BLOCK2 {}", command.len());
        let ret = connection.peek(&mut command)?;
        if ret == 0 {
            return Err(Error::new(ErrorKind::UnexpectedEof, "EOF"));
        }
        else if  ret < command.len() {
            //println!("YYEEET");
            return Err(Error::new(ErrorKind::WouldBlock, "would block"));
        }

        connection.read_exact(&mut command);

        if net_debug { 
            let opcode = command[0];
            let data = &command[8..(dl_index + 8)];

            println!("Received command (non-blocking): {:?} | {:?} | {:?}", opcode, data_len, data);}
        return Ok((command[0], data_len, command[8..(dl_index + 8)].to_vec()));
    }
}

//Send a command to a TCPStream
fn send_command(opcode : u8, data_len : u64, data : &Vec<u8>, connection : &mut TcpStream)->Result<()>{
     let net_debug = true;
     let mut data = data.clone();
     if net_debug { 
        println!("Sending command: {:?} | {:?} | {:?}", opcode, data_len, data);
     }
     //Add the headers to the outgoing data
     let be_bytes = data_len.to_be_bytes();
     data.insert(0, be_bytes[0]);
     data.insert(0, be_bytes[1]);
     data.insert(0, be_bytes[2]);
     data.insert(0, be_bytes[3]);
     data.insert(0, be_bytes[4]);
     data.insert(0, be_bytes[5]);
     data.insert(0, be_bytes[6]);
     data.insert(0, be_bytes[7]);
     data.insert(0, opcode);
     connection.write_all(&data);

     return Ok(());
}


