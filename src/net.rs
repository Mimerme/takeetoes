use std::io::{Error, ErrorKind};
use std::net::{TcpListener, TcpStream, Ipv4Addr, SocketAddrV4, SocketAddr, IpAddr};
use diffy::{apply, Patch, create_patch};
use sha2::{Sha256, Sha512, Digest};

//Network Opcodes. support for 255
#[repr(u8)]
enum NetOp {
    INTRO, INTRO_RES, AD, MSG, PING, PONG,
    PROJ, PROJ_VER, PATCH,
    VOTE_ASK, VOTE_RES
}

fn recv_command(connection : &mut TcpStream, block : bool)->Result<(u8, u64, Vec<u8>)>{
    let net_debug = true;
    let mut network_header = vec![0;9];

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

//Sends the syncronization packets to the peer network
fn send_sync(mut connection : &mut TcpStream, old_contents : &str, new_contents : &str, file_bufs : &mut HashMap<String, (String, String)>, net_path : &str, 
    version_history : &mut HashMap<[u8;64], (String, [u8;64])>){
    let patch = create_patch(old_contents, new_contents);

    //create_patch() doesn't allow us to set the 'original' and 'modified' headers so do it after its
    //created
    let mut patch = patch.to_string();
    patch = patch.replacen("original", net_path);
    patch = patch.replacen("modified", net_path);

    //Generate a hash id that corresponds to the file version that the patch should be applied to
    let mut old_hasher = Sha512::new();
    old_hasher.update(old_contents.as_bytes());
    old_hasher.update(net_path.as_bytes());
    let old_hash = old_hasher.finalize();
    old_hash.extend(patch.as_bytes());

    let mut new_hasher = Sha512::new();
    new_hasher.update(new_contents.as_bytes());
    new_hasher.update(net_path.as_bytes());

    println!("Patch\n========\n{:?}", patch);
    
    //Send the difference between two file versions
    send_command(NetOp::PATCH, old_hash.len() as u64, old_hash, &mut connection);

    //Update the project file buffer through the mutable reference
    file_bufs.get_mut(net_path.to_str()).unwrap().1 = new_contents.clone();
    //Update the version history
    //(update the commit related to the old contents [the link]
    //and insert the comit related to the new content)
    version_history.insert(&new_hasher.finalize(), (new_contents.clone(), None));
    version_history.get_mut(&old_hash.finalize()).1 = Some(&new_hasher.finalize());
}


fn verify_file_hash(proj_hash : &Vec<u8>, file_hash : &Vec<u8>, mut peer_stream : &mut TcpStream) -> bool{
      //Create the data vector to send
      let mut data = proj_hash.clone();
      data.extend(file_hash.clone());

      //Begin verifying the hash with the peer
      println!("Verifying hash with {:?}...", data);
      send_command(PROJ, 128, &data, &mut peer_stream);
      let (opcode, len, data) = recv_command(&mut peer_stream, true).unwrap();
      println!("PPO {:?}, {:?}, {:?}", opcode, len, data);
      
      if opcode != PROJ_VER || len != 1{
          panic!("ERR: Protocol out-of-order");
      }

      //1 = verified
      //0 = non-matching hashes
      //2 = willing to syncronize
      match data[0] {
         0 => {return false;},
         1 => {return true;},
         2 => {
            //Begin syncronization
            panic!("File syncronization needs implementing");
         },
        _ => {
            panic!("Protocol Err");
        }
      }
}
