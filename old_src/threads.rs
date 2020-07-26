//functions to spawn all the threads needed by the node
//NOTE: if you're looking for the debug thread it's in the main function
use notify::{Watcher, RecommendedWatcher, RecursiveMode};
use notify::Config;
use notify::event::{EventKind, ModifyKind,MetadataKind};
use crate::net::{NetOp};
use sha2::{Sha256, Sha512, Digest};
use std::collections::{HashSet, HashMap, VecDeque};
use std::{thread, time};
use std::sync::{Arc, Mutex, RwLock};
use std::sync::mpsc::{Sender, Receiver};

use crate::net::NetOp;

//Just sends an event on tx whenever the Metadata of a file in the directory changes
fn start_file_io(changes_out : Sender<(u8, u64, Patch)>, proj_dir : String, net_changes : Receiver<([u8;64], Patch)>, files : &mut HashMap<String, (String, String)>, 
    version_history : Arc<RwLock<HashMap<[u8;64], (String, [u8;64])>>>) {

    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher : RecommendedWatcher = Watcher::new_immediate(move |res| tx.send(res).unwrap()).unwrap();
    let mut change_buffer : HashMap<String, HashMap<[u8;64], (Patch, [u8;64])>> = HashMap::new();

    //water.configure(Config::PreciseEvents(true)).unwrap();
    watcher.watch(&proj_dir, RecursiveMode::Recursive).unwrap();
    
    loop {
        //On every time a file is written to in the directory we're monitoring...
        match rx.recv() {
            Ok(event) => {
                //Read in the patches to the change buffer so that whenever the file gets written
                //to the change queue is emptied
                while let Ok(change) = net_changes.try_recv() {
                    let hash = change.0;
                    let patch = change.1;
                    let net_path = patch.original();

                    //check to see if we need to insert the patch between two other patches
                    //
                    ////if None then we are fine
                    if change_buffer.get(net_path).get(hash) == None {
                        change_buffer.get(net_path).get_mut(hash).1 = 
                        let veresion_history = version_history.write().unwrap().ge 
                    }
                    //otherwise begin the rollback
                    else {
                    
                    }

                }


                let event = event.unwrap();
                if event.kind == EventKind::Modify(ModifyKind::Metadata(MetadataKind::Any)) {
                    println!("Meta change {:?}", event.paths);

                    //Check to see if in the file
                    for path in event.paths {
                        let net_path = path.strip_prefix(proj_dir.clone()).unwrap();
                        let net_path = net_path.to_str().unwrap().to_string();

                        //...if the file is part of the network...
                        if files.contains_key(&net_path) {
                            println!("writting change");

                            let host_path = files[&net_path].0.clone();
                            let mut old_contents = &files[&net_path].1.clone();
                            let mut new_contents = read_to_string(host_path.clone()).unwrap();

                            //TODO: working here
                            //NOTE: Andros, start working on moving some functions to different
                            //files to clean up this one and make it more readable
                            //...send the changes to the main network thread
                            let patch = create_patch(&old_contents, &new_contents);
                            let patch = patch.to_bytes();
                            changes_out.send((PATCH, patch.len(), patch));

                            println!("new contents\n{}", new_net_contents);
                            write(host_path.clone(), new_net_contents.as_bytes());
                            files.insert(net_path, (host_path, new_net_contents));
                        }
                    }
                }
            }, 
            Err(e) => println!("watch error: {:?}", e),
        }

    }
}



fn start_file_thread(changes_out : Sender<(u8, u64, Patch)>, proj_dir : String, net_changes : Receiver<(u8, u64, Vec<u8>)>, files : &mut Arc<RwLock<HashMap<String, (String, String)>>>, version_history : &mut Arc<RwLock<HashMap<String, (String, String)>>>) {
   //Start the file IO thread
   thread::spawn(move || {
       start_file_io(change_out, proj_dir, net_changes, &mut files.write().unwrap());
   });
}

fn start_network_thread() {
   //Thread to run the main event loop / protocol
   //This thread only deals with peers who have been learned / connected with
   thread::spawn(move || {
        let hashes = Arc::clone(&hashes);
        let hash_cache : HashSet<(Vec<u8>, Vec<u8>)> = HashSet::new();
        let ping_status = ping_status_clone;
        //Peers to remove on the next iteration of the loop
        let mut peers_to_remove :  HashSet<SocketAddr> = HashSet::new();

        println!("=====MAIN EVENT LOOP STARTED=====");

        loop{
            //Remove all the pears from the event loop
            if peers_to_remove.len() > 0 {
                for peer in peers_to_remove.iter() {
                    peers_clone.write().unwrap().remove(&peer);
                }
                peers_to_remove.drain();
            }

            //println!("{:?}", peers_clone.read().unwrap().iter().len());
            let mut peer_index = 0;
            //println!("{}", peers_clone.read().unwrap().iter().len());
            for (_, (read, write)) in peers_clone.read().unwrap().iter() {
                //send.lock().unwrap().send(b"asaaa".to_vec());
                let mut recv = read.lock().unwrap();
                match recv_command(&mut recv, false){
                    Ok((opcode, len, data)) => {
                        //Update the socket's entry in the ping table
                        ping_status.write().unwrap().insert(recv.peer_addr().unwrap(), (1, SystemTime::now()));

                        //Check for the opcode of the data first
                        match opcode {
                            NetOp::INTRO => {
                                println!("Received an INTRO");
                                //if verbose { println!("{} requested the peer list", string_ip);}
                                
                                let peers = peers_clone.read().unwrap();
                                let wr = write;
                                let mut write = wr.lock().unwrap();
                                //if verbose { println!("{:?}", peer_ads.read().unwrap());}
                               

                                let ad_ip_data = data;
                                //Format 'peer_ads' into bytes
                                let mut data = vec![];
                                for (_, peer) in peer_ads.read().unwrap().iter() {
                                    let mut ip = match peer.ip() {
                                        IpAddr::V4(ip) => ip.octets().to_vec(),
                                        IpAddr::V6(ip) => panic!("Protocol currently doesn't support ipv6 :("),
                                    };
                                    let port : u16 = peer.port();
                                    ip.push((port >> 8) as u8);
                                    ip.push(port as u8);
                                    data.extend(ip.iter());
                                }

                                send_command(INTRO_RES, (peer_ads.read().unwrap().len() * 6) as u8,
                                &data, &mut write);
                                if debug {println!("Responded with peer list");}

                                let port_number : u16 = ((ad_ip_data[4] as u16) << 8) | ad_ip_data[5] as u16;
                                let advertised_addr = SocketAddr::from(([ad_ip_data[0],
                                        ad_ip_data[1], ad_ip_data[2], ad_ip_data[3]], port_number));

                                peer_ads.write().unwrap().insert(write.peer_addr().unwrap(), advertised_addr);
                                if debug {println!("Added {:?} to the peer_list (INTRO)", advertised_addr);}
                            },
                            NetOp::AD => {
                                println!("SAVING AD");
                                let ad_ip_data = data;
                                let port_number : u16 = ((ad_ip_data[4] as u16) << 8) | ad_ip_data[5] as u16;
                                let advertised_addr = SocketAddr::from(([ad_ip_data[0],
                                        ad_ip_data[1], ad_ip_data[2], ad_ip_data[3]], port_number));

                                println!("Waiting for peer_ads");
                                //Udate the peer and advertisement list with the addr
                                peer_ads.write().unwrap().insert(write.lock().unwrap().peer_addr().unwrap(), advertised_addr.clone());

                                println!("UPDARTED!!");
                                if debug {println!("Added {:?} to the peer_list (AD)", advertised_addr);}
                            },
                            NetOp::MSG => {
                                println!("MSG: {}", String::from_utf8(data.to_vec()).unwrap());
                            },
                            NetOp::PING => {
                                if debug {println!("Received a PING");}
                                send_command(PONG, 0, &vec![], &mut write.lock().unwrap()); 
                                if debug {println!("Sent a PONG");}
                            }
                            NetOp::PONG => {
                                if debug {println!("Receivied a PONG");}
                                //Update the ping table
                                ping_status.write().unwrap().insert(recv.peer_addr().unwrap(), (1, SystemTime::now()));
                            },
                            NetOp::PROJ => {
                                println!("Responding to project verification");
                                if data.len() != 128 {
                                    println!("ERR: Received an invalid PROJ request");
                                }
                                let req_proj = data[0..64].to_vec();
                                let req_file = data[64..128].to_vec();

                                if debug {
                                    println!("RECV PROJ: {:?}", req_proj);
                                    println!("RECV FILE: {:?}", req_file);
                                }
                                let (proj_hash, file_hash) = get_directory_hash(project_dir.clone(), &mut HashMap::new(), false);

                                //TODO: implement syncronization
                                if proj_hash == req_proj && req_file == file_hash {
                                    send_command(PROJ_VER, 1, &vec![1], &mut write.lock().unwrap());
                                }
                                else{
                                    send_command(PROJ_VER, 1, &vec![0], &mut write.lock().unwrap());
                                }

                                let mut hashes = hashes.write().unwrap();
                                hashes.0 = proj_hash;
                                hashes.1 = file_hash;
                            },
                            NetOp::PATCH => {
                                let patch = String::from_utf8(&data[64..]).unwrap();
                                let hash = &data[0..64]; 
                                net_changes_in.send((hash, Patch::from_string(patch)));
                            },
                            _ => {
                                println!("??? Unknown OpCode ???: ({:?}, Length: {:?})", data, len);
                            }
                        }



                        //println!("{:?}", data);
                    },
                    Err(ref e) if e.kind() ==  ErrorKind::WouldBlock => {
                        let now = SystemTime::now();
                        let read_only = ping_status.read().unwrap().clone();


                        //Update the ping table if needed
                        for (host_sock, (status, last_command)) in read_only.iter() {
                            let seconds_elapsed = last_command.elapsed().unwrap().as_secs();

                            //If more than 10 seconds have elapsed since the last update then send
                            //a ping
                            if seconds_elapsed >= 60 && *status == 1u8{
                                if debug {println!("Sending a PING to {:?} (host socket) {:?}", host_sock, write.lock().unwrap());}
                                send_command(PING, 0, &vec![], &mut write.lock().unwrap());
                                ping_status.write().unwrap().insert(*host_sock, (2, SystemTime::now()));
                                println!("YEET");
                            }
                            //If more than 10 seconds have elapsed since the ping then assume the
                            //peer is dead
                            else if seconds_elapsed >= 60 && *status == 2u8 {
                                println!("NO REPSONSE FROM PEER. REMOVED");
                                peers_to_remove.insert(host_sock.clone());
                                //ping_status_clone.write().unwrap().remove(host_sock);
                                peer_ads.write().unwrap().remove(host_sock);
                                ping_status.write().unwrap().remove(host_sock);
                                //ping_status.write().unwrap().insert(*host_sock, (0, SystemTime::now()));
                                println!("DONE!");
                            }
                        }
                    },
                    Err(ref e) if e.kind() == ErrorKind::UnexpectedEof => {
                        println!("Peer has disconnected!");
                        let host_sock = recv.peer_addr().unwrap();

                        peers_to_remove.insert(host_sock.clone());
                        peer_ads.write().unwrap().remove(&host_sock);
                        ping_status.write().unwrap().remove(&host_sock);
                    }
                    Err(e) => {
                        panic!(e);
                        println!("REMOVING PERE");
                    }
                }
                peer_index += 1;
            }


            //Handle the file change events and sending file differences if there exists any
            while let Ok(change) = event_recv.try_recv() {
                //Begin serializing the changeset to the network
                match change {
                    NetChange::Start(net_path) => {
                        //TODO: lol do something about this
                        if net_path.len() > 255 {
                            panic!("NET PATH TOO LONG. I DIDN'T THINK OF WHAT TO DO");
                        }

                        for (_, (r,w)) in peers_clone.read().unwrap().iter() {
                            let mut connection = w.lock().unwrap();
                            send_command(START_SYNC, net_path.len() as u8, &net_path.as_bytes().to_vec(), &mut connection);
                        }
                    },
                    NetChange::Add(diff) => {
                        //Send the start index and the data to append
                        let mut data = Vec::new();
                        data.extend_from_slice(diff.as_bytes());
                        for (_, (r,w)) in peers_clone.read().unwrap().iter() {
                            let mut connection = w.lock().unwrap();
                            send_command(SYNC_ADD, (diff.len()).try_into().unwrap(), &data, &mut connection);
                        }
                    },
                    NetChange::Rem(diff) => {
                        //Send the start index and the data to append
                        let mut data = Vec::new();
                        data.extend_from_slice(diff.as_bytes());
                        for (_, (r,w)) in peers_clone.read().unwrap().iter() {
                            let mut connection = w.lock().unwrap();
                            send_command(SYNC_REM, (diff.len()).try_into().unwrap(), &data, &mut connection);
                        }
                    },
                    NetChange::Same(diff) => {
                        //Send the start index and the data to append
                        let mut data = Vec::new();
                        data.extend_from_slice(diff.as_bytes());
                        for (_, (r,w)) in peers_clone.read().unwrap().iter() {
                            let mut connection = w.lock().unwrap();
                            send_command(SYNC_SAME, (diff.len()).try_into().unwrap(), &data, &mut connection);
                        }
                    },
                    NetChange::End() => {
                        for (_, (r,w)) in peers_clone.read().unwrap().iter() {
                            let mut connection = w.lock().unwrap();
                            send_command(STOP_SYNC, 0, &vec![], &mut connection);
                        }
                    },

                }
            }
            thread::sleep(event_loop_delay);
       }
   });
}
