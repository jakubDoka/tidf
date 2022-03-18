/*use std::path::PathBuf;

use util::{
    meta_data::Deserialize,
    prelude::*,
    sprite_sheet::{self, SpriteData}, store::{Table, Access}, create_access,
};

pub mod map;

macro_rules! impl_id_des {
    ($($storage:ident => ($name:ident $(, $type:ty)?),)*) => {
        create_access!($($name)*);
        $(
            impl Deserialize<Assets> for $name {
                fn deserialize_into(&mut self, state: &mut Assets, node: util::meta_data::Yaml) -> Result<(), String> {
                    match node {
                        util::meta_data::Yaml::Scalar(name) => {
                            *self = state.$storage
                                .get(name)
                                .ok_or_else(||
                                    format!(concat!(stringify!($name), " '{}' not found"), node)
                                )?;

                        },
                        $(
                            util::meta_data::Yaml::Mapping(_) => {
                                let data = <$type>::deserialize(state, node)?;
                                state.$storage.push_anon(data);
                            }
                        )?
                        _ => return Err(format!(concat!("expected ", stringify!($name), " name, got {}"), node)),
                    }

                    Ok(())
                }
            }
        )*
    }
}

create_access!(
    Map
);

impl Deserialize<Assets> for Map {
    fn deserialize_into(&mut self, state: &mut Assets, node: util::meta_data::Yaml) -> Result<(), String> {
        match node {

            _ => Err(format!("expected mapping, got {:?}", node)),
        }
    }
}

impl_id_des!(
    regions => (Texture),
    healths => (Health, HealthData),
    hit_boxes => (HitBox, HitBoxData),
    damages => (Damage, DamageData),
    buildings => (Building, BuildingData),
);

#[derive(Meta, Default)]
#[meta_parser(Assets)]
pub struct HitBoxData {
    pub size: f32,
}

#[derive(Meta, Default)]
#[meta_parser(Assets)]
pub struct HealthData {
    pub max: i32,
    pub defense: i32,
    pub regeneration: i32,
    pub regeneration_delay: f32,
    pub regeneration_tick: f32,
}

#[derive(Meta, Default)]
#[meta_parser(Assets)]
pub struct DamageData {
    pub value: i32,
    pub piercing: i32,
}

#[derive(Meta, Default)]
#[meta_parser(Assets)]
pub struct BuildingData {
    pub base: Texture,
    pub health: Health,
    pub size: u32,
}


#[derive(Meta, Default)]
#[meta_parser(Assets)]
pub struct Bundle {
    pub map: Map,

}

pub struct Assets {
    pub texture: Image,
    pub regions: Table<Texture, Rectangle>,
    pub healths: Table<Health, HealthData>,
    pub hit_boxes: Table<HitBox, HitBoxData>,
    pub damages: Table<Damage, DamageData>,
    pub buildings: Table<Building, BuildingData>,
    pub maps: Table<Map, map::MapData>,
}

/// Game mechanics
impl Assets {
    /// Damage that is negative is considered healing
    pub fn calc_damage(&self, health: Health, damage: Damage) -> i32 {
        let defense = self.healths[health].defense;
        let DamageData { value, piercing } = self.damages[damage];
        value * (piercing >= defense) as i32 - value * (piercing < 0) as i32
    }
}

/// Asset loading
impl Assets {
    pub fn new<'a>(paths: &[&str]) -> Result<Self, String> {
        //# Load the textures and pack them
        let mut textures = vec![];

        // load textures
        walk_subdirectory(paths, ".png", &["textures"], |path| {
            let image = Image::load_image(&path)?;
            let image_data = SpriteData::new(path, image, false);
            textures.push(image_data);

            Ok(())
        })?;

        // pack textures
        let (texture, raw_regions) = sprite_sheet::new("textures", 5, &mut textures);
        let mut regions = Table::new();
        for (name, region) in raw_regions {
            regions.push(&name, region);
        }

        //# Load stats
        let mut assets;
        macro_rules! repeat_state_load {
            ($($name:ident)*) => {
                assets = Assets {
                    texture,
                    regions,
                    $($name: Table::new(),)*
                    maps: Table::new(),
                };

                $(
                    let mut $name = Table::new();
                    assets.load_stats(paths, stringify!($name), &mut $name)?;
                    assets.$name = $name;
                )*
            }
        }
        repeat_state_load!(
            healths
            damages
            hit_boxes
            buildings
        );

        Ok(assets)
    }

    pub fn texture(&self) -> &Image {
        &self.texture
    }

    pub fn region(&self, tex: Texture) -> Rectangle {
        self.regions[tex]
    }

    pub fn load_stats<K: Access, T: Deserialize<Self>>(
        &mut self,
        paths: &[&str],
        sub: &str,
        storage: &mut Table<K, T>,
    ) -> Result<(), String> {
        walk_subdirectory(paths, ".yaml", &["stats", sub], |path| {
            let content = std::fs::read_to_string(&path)
                .map_err(|err| format!("Failed to open {} file '{}': {}", sub, &path, err))?;
            let parsed = util::meta_data::parse(&content)
                .map_err(|err| format!("Failed to parse {} YAML file '{}': {}", sub, &path, err))?;
            storage
                .deserialize_into(self, parsed)
                .map_err(|err| format!("inside file '{}': {}", path, err))?;
            Ok(())
        })
    }
}

pub fn walk_subdirectory(
    paths: &[&str],
    ext: &str,
    sub: &[&str],
    mut processor: impl FnMut(String) -> Result<(), String>,
) -> Result<(), String> {
    let mut buffer = PathBuf::new();
    for path in paths {
        buffer.push(path);
        for segment in sub {
            buffer.push(segment);
        }
        if buffer.exists() {
            for entry in std::fs::read_dir(&buffer)
                .map_err(|err| format!("Failed to read dir '{}': {}", buffer.display(), err))?
            {
                let entry = entry.map_err(|err| format!("Failed to get file entry: {}", err))?;
                let path = entry.path();
                if !path.ends_with(ext) {
                    continue;
                }
                let path = path
                    .to_str()
                    .ok_or_else(|| {
                        format!("Path '{}' does not have UTF8 encoding.", path.display())
                    })?
                    .to_string();
                processor(path)?;
            }
        }
        buffer.clear();
    }

    Ok(())
}

pub fn convert_io(error: std::io::Error) -> String {
    error.to_string()
}
*/
