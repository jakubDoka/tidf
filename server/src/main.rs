#![feature(auto_traits)]
#![feature(negative_impls)]
#![feature(tcplistener_into_incoming)]

extern crate server;

fn main() {
    let mut server = server::server::Server::new(4, 30, 8080);

    server.run().unwrap();
}
