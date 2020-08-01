# Takeetoe
![Build Status](https://www.travis-ci.com/Mimerme/takeetoes.svg?branch=master)
## A runtime environment for quickly and easily creating small p2p network applications. 

## Summary
Takeetoe is a basic p2p network protocol that supports up to 42 users (changeable). Each node is connected to all other nodes. And there are no private p2p communications, everything is broadcasted.
"Takeetoe" also refers to the binary produced by this project
Applications built on top of the runtime can interface with a p2p network by connecting a TCP port on localhost where Takeetoe does inter-process IO.

## Usage
### Starts the initial Takeetoe Node
./takeetoe --binding_ip 127.0.0.1:8080

### Connects to the intial Takeetoe Node and sets up a server for others to connect to. Port forwarding is assumed to have been done manually
./takeetoe --binding_ip 127.0.0.1:9090 --connecting_ip 127.0.0.1:8080

### Connects to the intial Takeetoe Node and sets up a server for others to connect to. Port forwarding is done automatically using the UNPnP protocol.
./takeetoe --binding_ip 127.0.0.1:9090 --connecting_ip 127.0.0.1:8080 -unpnp

### Connects to the intial Takeetoe Node, which sets up a leecher client that uses 'connecting_ip' as a central server
./takeetoe --connecting_ip 127.0.0.1:8080

### Connects to the intial Takeetoe Node and sets up a server for others to connect to via NAT traversal. 'punch_ip' indicates connections to this server are established to 'binding_ip' using a redezvous server.
./takeetoe --binding_ip 127.0.0.1:9090 --connecting_ip 127.0.0.1:8080 --punch_ip 127.0.0.1:4242 --punch

### Broadcasting Data
nc localhost 12345 < echo "this is a test"
