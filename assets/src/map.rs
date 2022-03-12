use util::{prelude::*, pathfinder::TeamId};

use crate::*;

#[derive(Bitwise, Default)]
pub struct MapData {
    pub size: (u32, u32),
    pub tiles: Vec<Tile>,
}

#[derive(Clone, Copy, Default)]
pub struct Tile {
    pub texture: Texture,
    pub overlay: Texture,
    pub building: Building,
    pub team: TeamId,
}
