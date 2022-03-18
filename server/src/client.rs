use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream, UdpSocket, ToSocketAddrs},
    time::Duration,
};

use bitwise::{Bitwise, Decoder, Encoder};

use crate::protocol::{JoinRequestData, self, JoinInfo, Player};

pub struct Client {
    tcp: TcpStream,
    udp: UdpSocket,
    encoder: Encoder,
    decoder: Decoder,
    udp_addr: SocketAddr,
    join_info: JoinInfo,
}

impl Client {
    pub fn new(ip: &str, port: u16, join_request_data: JoinRequestData) -> std::io::Result<Self> {
        let mut tcp = TcpStream::connect((ip, port))?;
        let mut udp = UdpSocket::bind((ip, port))?;

        let mut encoder = Encoder::new();
        encoder.encode(&join_request_data);

        tcp.set_read_timeout(Some(Duration::new(3, 0)))?;
        tcp.write(encoder.data())?;
        tcp.set_read_timeout(None)?;

        let mut decoder = Decoder::new();
        protocol::read_tcp_packet_bytes(&mut tcp, &mut decoder)?;
        let join_info: JoinInfo = decoder.decode()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "Failed to parse join data."))?;

        let udp_addr = SocketAddr::new(ip.parse().unwrap(), join_info.udp_port); 
        encoder.clear();
        encoder.encode()
        udp.send_to(buf, addr)


        Ok(Self {
            tcp,
            udp,
            encoder,
            decoder,
            udp_addr,
            join_info,
        })
    }
}
