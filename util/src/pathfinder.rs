use std::sync::{
    mpsc::{self, Receiver, Sender},
    Arc,
};

use crate::sync::{DoubleState, Synchronize};

pub struct PathFinder {
    inner: Arc<DoubleState<InnerPathFinder>>,
    input: Sender<PathCommand>,
    output: Receiver<(bool, (i32, i32))>,
}

impl PathFinder {
    pub fn new() -> Self {
        let (input_s, input_r) = mpsc::channel();
        let (output_s, output_r) = mpsc::channel();
        let s = Self {
            inner: Arc::new(DoubleState::new(InnerPathFinder::new())),
            input: input_s,
            output: output_r,
        };
        {
            let s = s.inner.clone();
            std::thread::spawn(move || {
                while let Ok(command) = input_r.recv() {
                    match command {
                        PathCommand::Remap => drop(s.borrow_mut().remap()),
                        PathCommand::Place(location) => {
                            output_s
                                .send((s.borrow_mut().place(location), location))
                                .unwrap();
                        }
                        PathCommand::Remove(location) => s.borrow_mut().remove(location),
                    }
                }
            });
        }
        s
    }

    pub fn create_team(&self, x: i32, y: i32, mapping: Option<&Mapping>) -> TeamId {
        let mapping = self
            .inner
            .borrow()
            .defined_mapping()
            .or_else(|| mapping.cloned())
            .expect("No initial mapping defined.");
        let team = Team::new(x, y, mapping);
        let t = self.inner.borrow_mut().create_team(team);
        self.input.send(PathCommand::Remap).unwrap();
        t
    }

    pub fn remove_team(&self, team: TeamId) {
        self.inner.borrow_mut().remove_team(team);
        self.input.send(PathCommand::Remap).unwrap();
    }

    pub fn next_step(&self, team: TeamId, x: i32, y: i32) -> (i32, i32) {
        self.inner.borrow().next_step(team, x, y)
    }

    pub fn collect_place_requests(&self, buff: &mut Vec<(bool, (i32, i32))>) {
        buff.extend(self.output.try_iter());
    }
}

pub enum PathCommand {
    Remap,
    Place((i32, i32)),
    Remove((i32, i32)),
}

#[derive(Clone)]
pub struct InnerPathFinder {
    teams: Vec<Team>,
    free_teams: Vec<u32>,
}

impl InnerPathFinder {
    pub fn new() -> Self {
        Self {
            teams: Vec::new(),
            free_teams: Vec::new(),
        }
    }

    pub fn defined_mapping(&self) -> Option<Mapping> {
        self.teams.first().map(|t| t.mapping.clone())
    }

    pub fn remap(&mut self) -> bool {
        let mut frontier = vec![];
        let mut temp = vec![];

        (0..self.teams.len()).fold(true, |acc, i| {
            acc && {
                frontier.clear();
                frontier.extend(self.teams.iter().enumerate().filter_map(|(di, t)| {
                    if di != i {
                        None
                    } else {
                        Some(t.location)
                    }
                }));

                self.teams[i].remap(&mut frontier, &mut temp)
            }
        })
    }

    pub fn place(&mut self, location: (i32, i32)) -> bool {
        for team in self.teams.iter_mut() {
            team.place(location);
        }

        if !self.remap() {
            for team in self.teams.iter_mut() {
                team.remove(location);
            }
            assert!(self.remap());
            false
        } else {
            true
        }
    }

    pub fn remove(&mut self, location: (i32, i32)) {
        for team in self.teams.iter_mut() {
            team.remove(location);
        }

        assert!(self.remap());
    }

    pub fn create_team(&mut self, team: Team) -> TeamId {
        if let Some(free_team) = self.free_teams.pop() {
            self.teams[free_team as usize] = team;
            TeamId(free_team)
        } else {
            let team_id = self.teams.len() as u32;
            self.teams.push(team);
            TeamId(team_id)
        }
    }

    pub fn remove_team(&mut self, team_id: TeamId) -> Team {
        self.free_teams.push(team_id.0);
        std::mem::take(&mut self.teams[team_id.0 as usize])
    }

    pub fn next_step(&self, team: TeamId, x: i32, y: i32) -> (i32, i32) {
        let team = &self.teams[team.0 as usize];
        team.next_step(x, y)
    }
}

impl Synchronize for InnerPathFinder {
    fn synchronize(&mut self, other: &Self) {
        self.teams.resize(other.teams.len(), Team::default());
        self.teams
            .iter_mut()
            .zip(other.teams.iter())
            .for_each(|(s, o)| s.synchronize(o))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TeamId(u32);

impl Default for TeamId {
    fn default() -> Self {
        TeamId(u32::MAX)
    }
}

#[derive(Clone, Default)]
pub struct Team {
    location: (i32, i32),
    mapping: Mapping,
}

impl Team {
    pub fn new(x: i32, y: i32, mapping: Mapping) -> Self {
        Self {
            location: (x, y),
            mapping,
        }
    }

    fn next_step(&self, x: i32, y: i32) -> (i32, i32) {
        self.mapping.next_step(x, y)
    }

    fn remap(&mut self, frontier: &mut Vec<(i32, i32)>, temp: &mut Vec<(i32, i32)>) -> bool {
        self.mapping.remap(self.location, frontier, temp)
    }

    fn place(&mut self, location: (i32, i32)) {
        self.mapping.place(location)
    }

    fn remove(&mut self, location: (i32, i32)) {
        self.mapping.remove(location)
    }
}

impl Synchronize for Team {
    fn synchronize(&mut self, other: &Self) {
        self.location = other.location;
        self.mapping.synchronize(&other.mapping);
    }
}

#[derive(Clone, Default)]
pub struct Mapping {
    data: Vec<i32>,
    stride: i32,
}

impl Mapping {
    pub const UNEXPLORED: i32 = -1;
    pub const UNREACHABLE: i32 = -2;
    pub const STRAIGHT_DIRECTIONS: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
    pub const DIAGONAL_DIRECTIONS: [(i32, i32); 4] = [(1, 1), (1, -1), (-1, -1), (-1, 1)];

    pub fn new(width: usize, height: usize) -> Self {
        Self {
            data: vec![Self::UNEXPLORED; width * height],
            stride: height as i32,
        }
    }

    pub fn valid(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.stride as i32 && y < self.data.len() as i32 / self.stride
    }

    pub fn set(&mut self, (x, y): (i32, i32), value: i32) {
        self.data[(y * self.stride + x) as usize] = value;
    }

    pub fn get(&self, (x, y): (i32, i32)) -> i32 {
        self.data[(y * self.stride + x) as usize]
    }

    pub fn next_step(&self, x: i32, y: i32) -> (i32, i32) {
        let mut best_option = (i32::MAX, (x, y));

        for (dx, dy) in Self::STRAIGHT_DIRECTIONS.iter() {
            let nx = x + dx;
            let ny = y + dy;
            if self.valid(nx, ny) {
                let value = self.get((nx, ny));
                if value >= 0 && best_option.0 > value {
                    best_option = (value, (nx, ny));
                }
            }
        }

        for (dx, dy) in Self::DIAGONAL_DIRECTIONS.iter() {
            let nx = x + dx;
            let ny = y + dy;
            if self.valid(nx, ny) && self.valid(x, ny) && self.valid(nx, y) {
                let value = self.get((ny, ny));
                if value >= 0 && best_option.0 > value {
                    best_option = (value, (nx, ny));
                }
            }
        }

        best_option.1
    }

    fn remap(
        &mut self,
        target: (i32, i32),
        frontier: &mut Vec<(i32, i32)>,
        temp: &mut Vec<(i32, i32)>,
    ) -> bool {
        self.clear();

        let current = 0;
        while frontier.len() > 0 {
            for pos in frontier.drain(..) {
                for &(dx, dy) in Self::STRAIGHT_DIRECTIONS.iter() {
                    self.set((dx, dy), current);
                    let nx = pos.0 + dx;
                    let ny = pos.1 + dy;
                    if self.valid(nx, ny) {
                        let value = self.get((nx, ny));
                        if value == Self::UNEXPLORED {
                            temp.push((nx, ny));
                        }
                    }
                }
            }
            std::mem::swap(frontier, temp);
        }

        self.get(target) != Self::UNEXPLORED
    }

    fn clear(&mut self) {
        self.data
            .iter_mut()
            .filter(|&&mut i| i != Self::UNREACHABLE)
            .for_each(|v| *v = Self::UNEXPLORED);
    }

    fn place(&mut self, location: (i32, i32)) {
        self.set(location, Self::UNREACHABLE);
    }

    fn remove(&mut self, location: (i32, i32)) {
        self.set(location, Self::UNEXPLORED);
    }
}

impl Synchronize for Mapping {
    fn synchronize(&mut self, other: &Self) {
        self.stride = other.stride;
        self.data.clear();
        self.data.extend_from_slice(&other.data)
    }
}
