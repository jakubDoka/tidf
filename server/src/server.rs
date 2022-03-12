use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream, UdpSocket},
    ops::{Deref, DerefMut, Index, IndexMut},
    sync::{
        atomic::{AtomicI64, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

pub const ERROR_OC: u32 = 0;
pub const PLAYER_JOIN_OC: u32 = 1;
pub const JOIN_REQUEST_OC: u32 = 2;
pub const KICK_REQUEST_OC: u32 = 3;

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

pub auto trait NotReference {}

impl<'a, T> !NotReference for &'a T {}
impl<'a, T> !NotReference for &'a mut T {}

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
        let listener = TcpListener::bind(format!("127.0.0.1:{}", self.port))?;

        println!("Starting to listen udp at port {}!", self.port);

        for connection in listener.incoming() {
            match connection {
                Ok(conn) => self.handle_connection(conn),
                Err(e) => {
                    log!("Error when dispatching connections: {}", e);
                }
            }
        }

        Ok(())
    }

    pub fn handle_connection(&mut self, conn: TcpStream) {
        let mut player = PlayerEnt::new(conn);

        player.start_join_timeout();

        let request_data = match player.read_join_request() {
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
    sessions: PoolStorage<Session, SessionEnt>,
}

impl ThreadState {
    pub fn new(id: usize, port: u16, resources: Arc<AtomicI64>) -> Self {
        Self {
            id: id as u32,
            port: port + id as u16,
            resources,
            sessions: PoolStorage::new(),
        }
    }

    pub fn run(&mut self, fps: usize, mut new_connections: Receiver<JoinRequest>) {
        let mut limiter = FrameLimiter::new();
        let mut udp_reader = Buffer::new();
        let mut package_pool = vec![];
        let mut packages = vec![];
        let mut kick_queue = vec![];
        let mut close_queue = vec![];
        limiter.set_fps(fps);

        println!("Starting to listen udp on port {}!", self.port);
        let mut udp = UdpSocket::bind(format!("127.0.0.1:{}", self.port))
            .expect("Could not bind UDP socket!");
        udp.set_nonblocking(true).expect("Could not set nonblocking!");

        loop {
            self.collect_new_connections(&mut new_connections);

            match self.handle_udp_packets(
                &mut udp,
                &mut udp_reader,
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
                        .collect_tcp_packages(session_id, id, &mut package_pool, &mut packages)
                        .is_none()
                    {
                        kick_queue.push(id);
                    }
                }

                for package in &packages {
                    session.send_package(&package, &mut kick_queue, &mut udp);
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

    pub fn collect_new_connections(&mut self, new_connections: &mut Receiver<JoinRequest>) {
        for JoinRequest { mut player, data } in new_connections.try_iter() {
            log!("Connection arrived!",);
            if data.session == JoinRequestData::NEW_SESSION {
                self.create_session(data.password, player);
                continue;
            }

            if !self.sessions.is_valid(data.session) {
                log!(player.error("Session does not exists!"));
            }

            log!("Player is joining session {}", data.session.0);
            self.sessions[data.session].accept(self.id, data.session, self.port, data.password, player);
        }
    }

    pub fn create_session(&mut self, password: u128, player: PlayerEnt) {
        let session = SessionEnt::new(password, player);
        let owner = session.owner();
        let id = self.sessions.push(session);
        log!("Session created with id {}", id.0);
        log!(self.sessions[id].send_join_info(self.id, id, owner, self.port));
    }

    pub fn handle_udp_packets(
        &mut self,
        udp: &mut UdpSocket,
        buffer: &mut Buffer,
        kick_queue: &mut Vec<Player>,
        package_pool: &mut Vec<Package>,
    ) -> std::io::Result<()> {
        let mut size = [0u8; 4];
        loop {
            udp.peek(&mut size)?;
            let size = u32::from_le_bytes(size);
            let addr = buffer.load(size as usize, udp)?;
            let mut package = package_pool.pop().unwrap_or_default();
            if package.load(None, buffer).is_none() {
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
            if !player.set_udp_addr(addr) {
                log!(player.error("Udp and tcp ip does not match!"));
                package_pool.push(package);
                continue;
            }

            session.send_package(&package, kick_queue, udp);
            for kick in kick_queue.drain(..) {
                // no duplicates this time since we send
                // just one packet
                session.players.remove(kick);
            }

            package_pool.push(package);
        }
    }
}

#[derive(Debug, Default)]
pub struct Package {
    op_code: u32,
    source: Player,
    session: Session,
    tcp: bool,
    targets: Vec<Player>,
    data: Vec<u8>,
}

impl Package {
    pub fn load(&mut self, hint: Option<(Session, Player)>, buffer: &mut Buffer) -> Option<()> {
        self.op_code = buffer.read()?;
        self.source = buffer.read()?;
        self.session = buffer.read()?;
        if let Some((session, source)) = hint {
            if self.session != session || self.source != source {
                return None;
            }
        }
        self.tcp = buffer.read()?;

        self.targets.clear();
        buffer.read_into(&mut self.targets)?;

        self.data.clear();
        buffer.read_into(&mut self.data)?;

        Some(())
    }

    fn write(&self, buffer: &mut Buffer) {
        buffer.write(self.op_code);
        buffer.write(self.source);
        // not important to client
        //buffer.write(self.session);
        //buffer.write(self.tcp);
        //buffer.write(self.targets.as_slice());
        buffer.write(self.data.as_slice());
    }
}

pub struct SessionEnt {
    players: PoolStorage<Player, PlayerEnt>,
    password: u128,
    owner: Player,
}

impl SessionEnt {
    pub fn new(password: u128, owner: PlayerEnt) -> Self {
        let mut players = PoolStorage::new();
        let owner = players.push(owner);
        Self {
            players,
            password,
            owner,
        }
    }

    pub fn accept(&mut self, thread_id: u32, session: Session, udp_port: u16, password: u128, mut player: PlayerEnt) {
        if self.password != password {
            log!(player.error("Wrong password!"));
            return;
        }

        let id = self.players.push(player);
        if let Err(e) = self.send_join_info(thread_id, session, id, udp_port) {
            log!("failed to send join info: {}", e);
            self.players.remove(id);
        };
        log!("Session joined with id {}", id.0);
    }

    pub fn kick(&mut self, by: Player, target: Player) {
        if by != self.owner {
            log!(self.players[by].error("Only owner can kick!"));
            return;
        }

        log!(self.players.remove(target).error("You have been kicked!"));
    }

    pub fn send_join_info(&mut self, thread_id: u32, session: Session, joined: Player, udp_port: u16) -> std::io::Result<()> {
        self.players[joined].stop_blocking();

        for (_, player) in self.players.iter_mut() {
            player.send_join_info(thread_id, session, joined, udp_port)?;
        }

        Ok(())
    }

    fn send_package(&mut self, package: &Package, kick_queue: &mut Vec<Player>, udp: &mut UdpSocket) {
        match package.op_code {
            KICK_REQUEST_OC => {
                if package.targets.is_empty() {
                    log!(self.players[package.source].error("No target specified!"));
                } else {
                    self.kick(package.source, package.targets[0]);
                }
            }
            _ => (),
        }
        if package.targets.is_empty() {
            for (id, player) in self.players.iter_mut() {
                if package.source == id {
                    continue;
                }  
                
                if player.send_package(package, udp).is_none() {
                    kick_queue.push(id);
                }
            }
        } else {
            for &target in &package.targets {
                if self.players.is_valid(target) {
                    if self.players[target].send_package(package, udp).is_none() {
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

pub struct JoinRequestData {
    password: u128,
    session: Session,
    thread: u32,
}

impl JoinRequestData {
    pub const NEW_SESSION: Session = Session(u32::MAX);

    pub fn from_buffer(buffer: &mut Buffer) -> Option<Self> {
        Some(Self {
            password: buffer.read()?,
            session: buffer.read()?,
            thread: buffer.read()?,
        })
    }
}

pub struct PlayerEnt {
    last_packet: Instant,
    tcp: TcpStream,
    udp_addr: Option<SocketAddr>,
    buffer: Buffer,
}

impl PlayerEnt {
    pub fn new(tcp: TcpStream) -> Self {
        Self {
            last_packet: Instant::now(),
            tcp,
            udp_addr: None,
            buffer: Buffer::new(),
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
        pool: &mut Vec<Package>,
        packages: &mut Vec<Package>,
    ) -> Option<()> {
        loop {
            match self.recv_tcp() {
                Ok(_) => {
                    let mut package = pool.pop().unwrap_or_default();
                    package.load(Some((session, this)), &mut self.buffer)?;
                    self.buffer.clear();
                    packages.push(package);
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
        self.write(ERROR_OC);
        self.write(message);
        self.send(None)
    }

    fn send_join_info(&mut self, thread_id: u32, session: Session, joined: Player, udp_port: u16) -> std::io::Result<()> {
        self.write(PLAYER_JOIN_OC);
        self.write(thread_id);
        self.write(session);
        self.write(joined);
        self.write(udp_port);
        self.send(None)
    }

    fn send_package(&mut self, package: &Package, udp: &mut UdpSocket) -> Option<()> {
        package.write(&mut self.buffer);

        match self.send(if package.tcp { None } else { Some(udp) }) {
            Ok(_) => Some(()),
            Err(err) => {
                log!("failed to send package: {}", err);
                None
            }
        }
    }

    pub fn send(&mut self, udp: Option<&mut UdpSocket>) -> std::io::Result<()> {
        if let Some(udp) = udp {
            if let Some(addr) = self.udp_addr {
                udp.send_to(self.buffer.pack(false), addr)?;
            }
        } else {
            log!("sending tcp package to {}", self.tcp.peer_addr()?);
            self.tcp.write(self.buffer.pack(true))?;
        }
        self.buffer.clear();

        Ok(())
    }

    pub fn read_join_request(&mut self) -> Option<JoinRequestData> {
        self.recv_weak()?;
        // prevent spam
        const MAX_PACKAGE_SIZE: usize = std::mem::size_of::<(u32, JoinRequestData)>();
        if self.len() > MAX_PACKAGE_SIZE {
            log!(
                "initial package too big: {} > {}",
                self.len(),
                MAX_PACKAGE_SIZE
            );
            log!(self.error("Kicking for spamming!"));
            return None;
        }

        let op_code = self.read();

        if op_code != Some(JOIN_REQUEST_OC) {
            return None;
        }

        let req = JoinRequestData::from_buffer(&mut self.buffer);
        self.buffer.clear();
        req
    }

    pub fn recv_weak(&mut self) -> Option<()> {
        match self.recv_tcp() {
            Ok(()) => Some(()),
            Err(e) => {
                log!("failed to receive {}", e);
                None
            }
        }
    }

    pub fn recv_tcp(&mut self) -> std::io::Result<()> {
        let mut size = [0u8; 4];
        Read::read(&mut self.tcp, &mut size)?;
        let size = u32::from_le_bytes(size) as usize;
        self.buffer.load(size, &mut self.tcp)?;
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

impl Deref for PlayerEnt {
    type Target = Buffer;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl DerefMut for PlayerEnt {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffer
    }
}

#[derive(Debug, Clone)]
pub struct PoolStorage<K: PoolId, T> {
    data: Vec<Option<T>>,
    free: Vec<K>,
}

impl<K: PoolId, T> PoolStorage<K, T> {
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            free: Vec::new(),
        }
    }

    pub fn push(&mut self, data: T) -> K {
        if let Some(id) = self.free.pop() {
            self.data[id.index()] = Some(data);
            id
        } else {
            let id = self.data.len();
            self.data.push(Some(data));
            K::new(id)
        }
    }

    pub fn _clear(&mut self) {
        self.data.clear();
        self.free.clear();
    }

    pub fn remove(&mut self, id: K) -> T {
        let removed = self.data[id.index()].take().expect("double free");
        self.free.push(id);
        removed
    }

    pub fn is_valid(&self, id: K) -> bool {
        id.index() < self.data.len() && self.data[id.index()].is_some()
    }

    pub fn _iter(&self) -> impl Iterator<Item = (K, &T)> {
        self.data
            .iter()
            .enumerate()
            .filter_map(|(i, x)| x.as_ref().map(|x| (K::new(i), x)))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (K, &mut T)> {
        self.data
            .iter_mut()
            .enumerate()
            .filter_map(|(i, x)| x.as_mut().map(|x| (K::new(i), x)))
    }

    fn count(&self) -> usize {
        self.data.len() - self.free.len()
    }
}

impl<K: PoolId, T> Index<K> for PoolStorage<K, T> {
    type Output = T;

    fn index(&self, index: K) -> &Self::Output {
        self.data[index.index()].as_ref().expect("invalid index")
    }
}

impl<K: PoolId, T> IndexMut<K> for PoolStorage<K, T> {
    fn index_mut(&mut self, index: K) -> &mut Self::Output {
        self.data[index.index()].as_mut().expect("invalid index")
    }
}

macro_rules! impl_pool_id {
    ($($name:ident),*) => {
        $(
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
            pub struct $name(u32);
    
            impl PoolId for $name {
                fn new(index: usize) -> Self {
                    Self(index as u32)
                }
    
                fn index(&self) -> usize {
                    self.0 as usize
                }
            }
        )*
    };
}

impl_pool_id!(Player, Session);

pub trait PoolId: Clone + Copy + PartialEq + Eq + Default {
    fn new(index: usize) -> Self;
    fn index(&self) -> usize;
}