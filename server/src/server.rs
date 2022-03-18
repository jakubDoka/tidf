use std::{
    cell::RefCell,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream, UdpSocket},
    sync::{
        atomic::{AtomicI64, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use crate::protocol::{JoinInfo, JoinRequestData, Packet, Player, Session};
use bitwise::*;
use store::PoolStore;

macro_rules! log {
    ($template:literal, $($arg:expr),*) => {
        {
            #[cfg(debug_assertions)]
            {
                eprintln!($template, $($arg),*);
            }
        }
    };
    ($result:expr) => {
        {
            let _result = $result;
            #[cfg(debug_assertions)]
            {
                if let Err(e) = _result {
                    eprintln!("{}", e);
                }
            }
        }
    };
}

pub struct Server {
    port: u16,
    threads: Vec<ThreadHandle>,
}

impl Server {
    pub fn new(thread_count: usize, fps: usize, port: u16) -> Self {
        let mut threads = Vec::with_capacity(thread_count as usize);
        for i in 0..thread_count {
            let (sender, receiver) = mpsc::channel();
            let resources = Arc::new(AtomicI64::new(0));
            let mut state = ThreadState::new(i, port, resources.clone());
            let handle = thread::spawn(move || state.run(fps, receiver));
            let handle = ThreadHandle::new(sender, resources, handle);
            threads.push(handle);
        }

        Server { port, threads }
    }

    pub fn run(&mut self) -> std::io::Result<()> {
        let listener = TcpListener::bind(("127.0.0.1", self.port))?;
        let mut decoder = Decoder::new();

        println!("Starting to listen udp at port {}!", self.port);

        for connection in listener.incoming() {
            match connection {
                Ok(conn) => self.handle_connection(&mut decoder, conn),
                Err(e) => {
                    log!("Error when dispatching connections: {}", e);
                }
            }
        }

        Ok(())
    }

    pub fn handle_connection(&mut self, decoder: &mut Decoder, conn: TcpStream) {
        let mut player = PlayerEnt::new(conn);

        player.start_join_timeout();

        let request_data = match player.read_join_request(decoder) {
            Some(data) => data,
            None => {
                log!(player.error("Join request in invalid format!"));
                return;
            }
        };

        let mut best = request_data.thread as usize;
        // means that player is creating session
        if best == u32::MAX as usize {
            let mut best_resources = i64::MIN;
            for (i, thread) in self.threads.iter().enumerate() {
                let resources = thread.resources.load(Ordering::Relaxed);
                if resources > best_resources {
                    best = i;
                    best_resources = resources;
                }
            }
        }

        log!("sending connection to thread {}", best);
        self.threads[best]
            .new_connections
            .send(JoinRequest::new(player, request_data))
            .unwrap();
    }
}

pub struct ThreadHandle {
    resources: Arc<AtomicI64>,
    new_connections: Sender<JoinRequest>,
    _handle: thread::JoinHandle<()>,
}

impl ThreadHandle {
    pub fn new(
        new_connections: Sender<JoinRequest>,
        resources: Arc<AtomicI64>,
        handle: thread::JoinHandle<()>,
    ) -> Self {
        ThreadHandle {
            resources,
            new_connections,
            _handle: handle,
        }
    }
}

pub struct ThreadState {
    id: u32,
    port: u16,
    resources: Arc<AtomicI64>,
    sessions: PoolStore<Session, SessionEnt>,
}

impl ThreadState {
    pub fn new(id: usize, port: u16, resources: Arc<AtomicI64>) -> Self {
        Self {
            id: id as u32,
            port: port + id as u16,
            resources,
            sessions: PoolStore::new(),
        }
    }

    pub fn run(&mut self, fps: usize, mut new_connections: Receiver<JoinRequest>) {
        let mut limiter = FrameLimiter::new();
        let mut decoder = Decoder::new();
        let mut encoder = Encoder::new();
        let mut package_pool = vec![];
        let mut packages = vec![];
        let mut kick_queue = vec![];
        let mut close_queue = vec![];
        limiter.set_fps(fps);

        println!("Starting to listen udp on port {}!", self.port);
        let mut udp = UdpSocket::bind(format!("127.0.0.1:{}", self.port))
            .expect("Could not bind UDP socket!");
        udp.set_nonblocking(true)
            .expect("Could not set nonblocking!");

        loop {
            self.collect_new_connections(&mut encoder, &mut new_connections);

            match self.collect_udp_packets(
                &mut udp,
                &mut decoder,
                &mut encoder,
                &mut kick_queue,
                &mut package_pool,
            ) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => (),
                Err(e) => {
                    log!("Error when handling udp packets: {}", e);
                }
            };

            for (session_id, session) in self.sessions.iter_mut() {
                for (id, player) in session.players.iter_mut() {
                    if player
                        .collect_tcp_packages(
                            session_id,
                            id,
                            &mut package_pool,
                            &mut packages,
                            &mut decoder,
                        )
                        .is_none()
                    {
                        kick_queue.push(id);
                    }
                }

                for package in &packages {
                    session.send_package(&mut encoder, &package, &mut kick_queue, &mut udp);
                }
                package_pool.append(&mut packages);

                for kick in kick_queue.drain(..) {
                    // there can be duplicates
                    if session.players.is_valid(kick) {
                        session.players.remove(kick);
                    }
                }

                if session.players.count() == 0 {
                    close_queue.push(session_id);
                }
            }

            for id in close_queue.drain(..) {
                self.sessions.remove(id);
            }

            self.resources.store(limiter.update(), Ordering::Relaxed);
        }
    }

    pub fn collect_new_connections(
        &mut self,
        encoder: &mut Encoder,
        new_connections: &mut Receiver<JoinRequest>,
    ) {
        for JoinRequest { mut player, data } in new_connections.try_iter() {
            log!("Connection arrived!",);
            if data.session == JoinRequestData::NEW_SESSION_ID {
                self.create_session(encoder, data.password, player);
                continue;
            }

            if !self.sessions.is_valid(data.session) {
                log!(player.error("Session does not exists!"));
            }

            log!("Player is joining session {}", data.session.0);
            self.sessions[data.session].accept(
                encoder,
                self.id,
                data.session,
                self.port,
                data.password,
                player,
            );
        }
    }

    pub fn create_session(&mut self, encoder: &mut Encoder, password: u128, player: PlayerEnt) {
        let session = SessionEnt::new(password, player);
        let joined = session.owner();
        let session = self.sessions.push(session);
        log!("Session created with id {}", session.0);
        encoder.encode(&JoinInfo {
            session,
            joined,
            thread_id: self.id,
            udp_port: self.port,
        });
        log!(self.sessions[session].send_join_info(joined, encoder));
        encoder.clear();
    }

    pub fn collect_udp_packets(
        &mut self,
        udp: &mut UdpSocket,
        decoder: &mut Decoder,
        encoder: &mut Encoder,
        kick_queue: &mut Vec<Player>,
        package_pool: &mut Vec<Packet>,
    ) -> std::io::Result<()> {
        let mut size = [0u8; 4];
        loop {
            udp.peek(&mut size)?;
            let size = u32::from_le_bytes(size);
            let (_, addr) = udp.recv_from(decoder.expose(size as usize + Encoder::LEN_SIZE))?;
            decoder.decode::<u32>();
            let mut package = package_pool.pop().unwrap_or_default();
            if decoder.decode_into(&mut package).is_none() {
                package_pool.push(package);
                continue;
            }

            if !self.sessions.is_valid(package.session) {
                log!("Invalid session id {}!", package.session.0);
                package_pool.push(package);
                continue;
            }

            let session = &mut self.sessions[package.session];
            if !session.players.is_valid(package.source) {
                log!(
                    "Player {} is not in session {}!",
                    package.source.0,
                    package.session.0
                );
                package_pool.push(package);
                continue;
            }

            let player = &mut session.players[package.source];
            if !player.set_udp_addr(Some(addr)) {
                log!(player.error("Udp and tcp ip does not match!"));
                package_pool.push(package);
                continue;
            }

            session.send_package(encoder, &package, kick_queue, udp);
            for kick in kick_queue.drain(..) {
                // no duplicates this time since we send
                // just one packet
                session.players.remove(kick);
            }

            package_pool.push(package);
        }
    }
}

pub struct SessionEnt {
    players: PoolStore<Player, PlayerEnt>,
    password: u128,
    owner: Player,
}

impl SessionEnt {
    pub fn new(password: u128, owner: PlayerEnt) -> Self {
        let mut players = PoolStore::new();
        let owner = players.push(owner);
        Self {
            players,
            password,
            owner,
        }
    }

    pub fn accept(
        &mut self,
        encoder: &mut Encoder,
        thread_id: u32,
        session: Session,
        udp_port: u16,
        password: u128,
        mut player: PlayerEnt,
    ) {
        if self.password != password {
            log!(player.error("Wrong password!"));
            return;
        }

        let joined = self.players.push(player);
        encoder.encode(&JoinInfo {
            thread_id,
            session,
            joined,
            udp_port,
        });
        if let Err(e) = self.send_join_info(joined, encoder) {
            log!("failed to send join info: {}", e);
            self.players.remove(joined);
        };
        log!("Session joined with id {}", joined.0);
    }

    pub fn kick(&mut self, by: Player, target: Player) {
        if by != self.owner {
            log!(self.players[by].error("Only owner can kick!"));
            return;
        }

        log!(self.players.remove(target).error("You have been kicked!"));
    }

    pub fn send_join_info(&mut self, joined: Player, encoder: &mut Encoder) -> std::io::Result<()> {
        self.players[joined].stop_blocking();
        for player in self.players.values_mut() {
            player.send(encoder, &None)?;
        }
        Ok(())
    }

    fn send_package(
        &mut self,
        encoder: &mut Encoder,
        data: &Packet,
        kick_queue: &mut Vec<Player>,
        udp: &mut UdpSocket,
    ) {
        match data.op_code {
            KICK_REQUEST_OC => {
                if data.targets.is_empty() {
                    log!(self.players[data.source].error("No target specified!"));
                } else {
                    self.kick(data.source, data.targets[0]);
                }
            }
            _ => (),
        }

        encoder.encode(data);
        let udp = if data.tcp { None } else { Some(udp) };

        if data.targets.is_empty() {
            for (id, player) in self.players.iter_mut() {
                if data.source == id {
                    continue;
                }

                if player.send_packet(encoder, &udp).is_none() {
                    kick_queue.push(id);
                }
            }
        } else {
            for &target in &data.targets {
                if self.players.is_valid(target) {
                    if self.players[target].send_packet(encoder, &udp).is_none() {
                        kick_queue.push(target);
                    }
                }
            }
        }
    }

    pub fn owner(&self) -> Player {
        self.owner
    }
}

pub struct FrameLimiter {
    fps: u32,
    time: Instant,
}

impl FrameLimiter {
    pub fn new() -> Self {
        Self {
            fps: 60,
            time: Instant::now(),
        }
    }

    pub fn set_fps(&mut self, fps: usize) {
        self.fps = fps as u32;
    }

    pub fn update(&mut self) -> i64 {
        let frame = 1_000_000_000 / self.fps;
        self.time += Duration::new(0, frame);
        let now = Instant::now();
        if now < self.time {
            let spare_time = self.time - now;
            thread::sleep(spare_time);
            return spare_time.subsec_nanos() as i64;
        }
        return -((now - self.time).subsec_nanos() as i64);
    }
}

pub struct JoinRequest {
    player: PlayerEnt,
    data: JoinRequestData,
}

impl JoinRequest {
    pub fn new(player: PlayerEnt, data: JoinRequestData) -> Self {
        JoinRequest { player, data }
    }
}

pub struct PlayerEnt {
    last_packet: Instant,
    tcp: TcpStream,
    udp_addr: Option<SocketAddr>,
}

impl PlayerEnt {
    pub fn new(tcp: TcpStream) -> Self {
        Self {
            last_packet: Instant::now(),
            tcp,
            udp_addr: None,
        }
    }

    pub fn set_udp_addr(&mut self, addr: Option<SocketAddr>) -> bool {
        self.udp_addr = addr;
        // better then nothing
        self.tcp.peer_addr().map(|addr| addr.ip()).ok() == addr.map(|addr| addr.ip())
    }

    pub fn is_inactive(&self) -> bool {
        self.last_packet.elapsed() > Duration::from_secs(60 * 10)
    }

    pub fn collect_tcp_packages(
        &mut self,
        session: Session,
        this: Player,
        pool: &mut Vec<Packet>,
        packages: &mut Vec<Packet>,
        decoder: &mut Decoder,
    ) -> Option<()> {
        loop {
            match self.recv_tcp(decoder, None) {
                Ok(_) => {
                    let mut packet = pool.pop().unwrap_or_default();
                    decoder.decode_into(&mut packet)?;
                    if packet.session != session || packet.source == this {
                        log!("invalid packet: {:?}", packet);
                        continue;
                    }
                    packages.push(packet);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if self.is_inactive() {
                        log!(self.error("Kicking for inactivity!"));
                        return None;
                    } else {
                        return Some(());
                    }
                }
                Err(err) => {
                    log!("{}", err);
                    log!(self.error("Kicking for sending garbage!"));
                    return None;
                }
            }
        }
    }

    pub fn error(&mut self, message: &str) -> std::io::Result<()> {
        log!("error sent to {}: {}", self.tcp.peer_addr()?, message);
        thread_local! {
            static ERROR_ENCODER: RefCell<Encoder> = RefCell::new(Encoder::new());
        }

        ERROR_ENCODER.with(|encoder| {
            let mut encoder = encoder.borrow_mut();
            encoder.assert_empty();
            encoder.encode(&ERROR_OC);
            encoder.encode_str(message);
            self.send(&mut *encoder, &None)?;
            encoder.clear();
            Ok(())
        })
    }

    fn send_packet(&mut self, data: &mut Encoder, udp: &Option<&mut UdpSocket>) -> Option<()> {
        match self.send(data, udp) {
            Err(err) => {
                log!("failed to send package: {}", err);
                None
            }
            _ => Some(()),
        }
    }

    pub fn send(
        &mut self,
        encoder: &mut Encoder,
        udp: &Option<&mut UdpSocket>,
    ) -> std::io::Result<()> {
        if let Some(udp) = udp {
            if let Some(addr) = self.udp_addr {
                let err = udp.send_to(encoder.data(), addr);
                encoder.clear();
                err?;
            }
        } else {
            log!("sending tcp package to {}", self.tcp.peer_addr()?);
            let err = self.tcp.write(encoder.data());
            encoder.clear();
            err?;
        }

        Ok(())
    }

    pub fn read_join_request(&mut self, decoder: &mut Decoder) -> Option<JoinRequestData> {
        self.recv_tcp_weak(decoder, Some(std::mem::size_of::<(u32, JoinRequestData)>()))?;

        let op_code = decoder.decode();

        if op_code != Some(JOIN_REQUEST_OC) {
            return None;
        }

        decoder.decode()
    }

    pub fn recv_tcp_weak(&mut self, decoder: &mut Decoder, max_size: Option<usize>) -> Option<()> {
        match self.recv_tcp(decoder, max_size) {
            Ok(()) => Some(()),
            Err(e) => {
                log!("failed to receive {}", e);
                None
            }
        }
    }

    pub fn recv_tcp(
        &mut self,
        decoder: &mut Decoder,
        max_size: Option<usize>,
    ) -> std::io::Result<()> {
        let mut size = [0u8; 4];
        Read::read(&mut self.tcp, &mut size)?;
        let size = u32::from_le_bytes(size) as usize;
        if max_size.map(|m| m < size).unwrap_or(false) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "package bigger then expected",
            ));
        }
        self.tcp.read(decoder.expose(size))?;
        self.last_packet = Instant::now();
        Ok(())
    }

    pub fn stop_blocking(&mut self) {
        log!(self.tcp.set_read_timeout(None));
        log!(self.tcp.set_nonblocking(true));
    }

    fn start_join_timeout(&mut self) {
        log!(self.tcp.set_read_timeout(Some(Duration::from_secs(1))));
    }
}
