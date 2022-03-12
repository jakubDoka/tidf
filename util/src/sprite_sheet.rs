use std::ops::Range;

use crate::prelude::*;

pub fn new<T: Packable + Sprite>(
    root_segment: &str,
    spacing: usize,
    data: &mut Vec<T>,
) -> (Image, Vec<(String, Rectangle)>) {
    let (width, height) = pack(data, spacing as i32);

    let mut texture = Image::gen_image_color(width, height, Color::default());
    for sprite in data.iter_mut() {
        let region = sprite.frame();
        let flip = sprite.flip();
        let image = sprite.image();

        if flip {
            image.flip_vertical();
        }

        texture.draw(image, image.bounds(), region, Color::WHITE);

        if flip {
            image.flip_vertical();
        }
    }

    let mut sprites = Vec::with_capacity(data.len());
    data.drain(..)
        .for_each(|s| s.into(root_segment, &mut sprites));

    (texture, sprites)
}

pub fn pack<T: Packable>(data: &mut [T], spacing: i32) -> (i32, i32) {
    data.sort_by(|a, b| b.height().cmp(&a.height()));

    let mut final_width = 0i32;
    let mut final_height = 0i32;

    bin_search(0..data.len(), |stride| {
        let mut last_sprite_height = data[0].height();
        let width = data[..stride].iter().map(|x| x.width()).sum::<i32>() + spacing * stride as i32;
        let mut x = spacing;
        let mut y = spacing;
        let mut i = 0;
        while i < data.len() {
            let sprite = &mut data[i];
            if (sprite.width() + x) > width {
                if x == spacing {
                    for sprite in &mut data[..i] {
                        sprite.recover();
                    }
                    return true;
                }
                x = spacing;
                y += last_sprite_height + spacing;
                last_sprite_height = sprite.height();
                continue;
            }
            sprite.set_pos(x, y);
            i += 1;
            x += sprite.width() + spacing;
        }

        let height = y + spacing * 2;
        let width = width + spacing;

        if width < height {
            final_width = width;
            final_height = height;
            false
        } else {
            for sprite in data.iter_mut() {
                sprite.recover();
            }
            true
        }
    });

    (final_width, final_height)
}

pub struct SpriteData {
    name: String,
    image: Image,
    pos: (i32, i32),
    saved: (i32, i32),
    flip: bool,
}

impl SpriteData {
    pub fn new(name: String, image: Image, flip: bool) -> Self {
        Self {
            name,
            image,
            pos: (0, 0),
            saved: (0, 0),
            flip,
        }
    }
}

impl Packable for SpriteData {
    fn width(&self) -> i32 {
        self.image.width()
    }

    fn height(&self) -> i32 {
        self.image.height()
    }

    fn set_pos(&mut self, x: i32, y: i32) {
        self.saved = self.pos;
        self.pos = (x, y);
    }

    fn recover(&mut self) {
        self.pos = self.saved;
    }

    fn x(&self) -> i32 {
        self.pos.0
    }

    fn y(&self) -> i32 {
        self.pos.1
    }
}

impl Sprite for SpriteData {
    fn into(mut self, root_segment: &str, buffer: &mut Vec<(String, Rectangle)>) {
        strip_path_garbage(&mut self.name, root_segment);
        if self.name.matches("_").count() == 2 {
            let mut parts = self.name.split("_");
            let name = parts.next().unwrap();

            // sprite sheet
            if let Ok(w) = parts.next().unwrap().parse::<u32>() {
                if let Ok(h) = parts.next().unwrap().parse::<u32>() {
                    let sx = self.x();
                    let sy = self.y();
                    let sw = self.width();
                    let sh = self.height();

                    let mut i = 1;
                    for y in (0..sh).step_by(h as usize) {
                        for x in (0..sw).step_by(w as usize) {
                            let region = Rectangle::new(
                                (sx + x) as f32,
                                (sy + y) as f32,
                                w as f32,
                                h as f32,
                            );
                            let name = format!("{}{}", name, i);
                            buffer.push((name, region));
                            i += 1;
                        }
                    }

                    return;
                }
            }
        }
        let x = self.pos.0 as f32;
        let y = self.pos.1 as f32;
        let region = Rectangle::new(x, y, self.image.width() as f32, self.image.height() as f32);
        buffer.push((self.name, region));
    }

    fn image(&mut self) -> &mut Image {
        &mut self.image
    }

    fn flip(&self) -> bool {
        self.flip
    }
}

pub trait Sprite {
    fn into(self, root_segment: &str, buffer: &mut Vec<(String, Rectangle)>);
    fn image(&mut self) -> &mut Image;
    fn flip(&self) -> bool;
}

pub trait Packable {
    fn x(&self) -> i32;
    fn y(&self) -> i32;
    fn width(&self) -> i32;
    fn height(&self) -> i32;

    fn set_pos(&mut self, x: i32, y: i32);
    fn recover(&mut self);

    fn frame(&self) -> Rectangle {
        let x = self.x() as f32;
        let y = self.y() as f32;
        Rectangle::new(x, y, self.width() as f32, self.height() as f32)
    }

    fn project(&mut self, height: i32) {
        self.set_pos(self.x(), height - self.y() - self.height());
    }
}

pub fn bin_search<F: FnMut(usize) -> bool>(mut range: Range<usize>, mut f: F) {
    while range.start < range.end {
        let mid = (range.start + range.end) >> 1;
        if f(mid) {
            range.end = mid;
        } else {
            range.start = mid + 1;
        }
    }
}

pub fn strip_path_garbage(path: &mut String, root_segment: &str) {
    if !root_segment.is_empty() {
        if let Some(idx) = path.rfind(root_segment) {
            path.replace_range(..idx + root_segment.len() + 1, "");
        }
    }
    if let Some(idx) = path.rfind('.') {
        path.truncate(idx);
    }
}

#[cfg(test)]
mod test {
    use rand::Rng;
    use raylib::{color::Color, texture::Image};

    use super::SpriteData;

    #[test]
    fn pack() {
        let mut data = vec![];

        let mut rnd = rand::thread_rng();

        for _i in 0..1000 {
            data.push(Image::gen_image_color(
                rnd.gen_range(1..10),
                rnd.gen_range(1..10),
                Color::WHITE,
            ));
        }

        let mut refs = data
            .into_iter()
            .enumerate()
            .map(|(i, e)| SpriteData::new(i.to_string(), e, false))
            .collect::<Vec<_>>();

        let now = std::time::Instant::now();
        let (texture, _) = super::new("thing", 10, &mut refs);
        println!("packed in: {}s", now.elapsed().as_secs_f64());
        texture.export_image("src/sheet_test.png");
    }
}
