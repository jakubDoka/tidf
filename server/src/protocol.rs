use std::{net::{TcpStream, UdpSocket, SocketAddr}, io::Read};

pub use bitwise::*;

store::create_access!(Player Session);

#[derive(Bitwise, Debug)]
pub enum OPCode {
    None,
    JoinGame,
    Main,
}

impl Default for OPCode {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Bitwise, Debug, Default)]
pub struct Packet {
    pub op_code: u32,
    pub session: Session,
    pub source: Player,
    pub tcp: bool,
    pub targets: Vec<Player>,
    pub data: Vec<u8>,
}

#[derive(Bitwise, Debug, Default)]
pub struct ServerPacket {
    pub op_code: u32,
    pub source: Player,
    pub data: Vec<u8>,
}

#[derive(Bitwise, Debug, Default)]
pub struct JoinInfo {
    pub thread_id: u32,
    pub session: Session,
    pub joined: Player,
    pub udp_port: u16,
}

#[derive(Bitwise, Debug, Default)]
pub struct JoinRequestData {
    pub password: u128,
    pub session: Session,
    pub thread: u32,
}

impl JoinRequestData {
    pub const NEW_SESSION_ID: Session = Session(u32::MAX);

    pub fn create(password: u128) -> Self {
        Self {
            password,
            session: Self::NEW_SESSION_ID,
            thread: u32::MAX,
        }
    }
}

pub fn read_tcp_packet_bytes(tcp: &mut TcpStream, into: &mut Decoder) -> std::io::Result<()> {
    let mut length = [0; 4];
    tcp.read(&mut length)?;
    let length = u32::from_le_bytes(length);
    tcp.read(into.expose(length as usize))?;
    Ok(())
}

pub fn read_udp_packet_bytes(udp: &mut UdpSocket, into: &mut Decoder) -> std::io::Result<SocketAddr> {
    let mut length = [0; 4];
    udp.peek(&mut length)?;
    let length = u32::from_le_bytes(length);
    let (_, addr) = udp.recv_from(into.expose(length as usize + 4))?;
    Ok(addr)
}