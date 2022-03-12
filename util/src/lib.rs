#![feature(auto_traits)]
#![feature(negative_impls)]
#![feature(ptr_internals)]

use raylib::prelude::*;

pub mod meta_data;
pub mod pathfinder;
pub mod quad_tree;
pub mod sprite_sheet;
pub mod sync;
pub mod store;

pub mod prelude {
    pub use crate::{
        meta_data::Deserialize, ImageExtension, RaylibDrawHandleExtension,
        RectangleExtension, Vector2Extension,
    };
    pub use derive::Meta;
    pub use raylib::prelude::*;
}

#[macro_export]
macro_rules! cstr {
    ($s:literal) => {
        unsafe { std::ffi::CStr::from_bytes_with_nul_unchecked(concat!($s, "\0").as_bytes()) }
    };
}

pub trait RaylibDrawHandleExtension {
    fn get_screen_rect(&self) -> Rectangle;
    fn draw_centered_text(&mut self, text: &str, position: Vector2, font_size: f32, color: Color);
}

impl RaylibDrawHandleExtension for RaylibDrawHandle<'_> {
    fn get_screen_rect(&self) -> Rectangle {
        Rectangle::new(
            0.0,
            0.0,
            self.get_screen_width() as f32,
            self.get_screen_height() as f32,
        )
    }

    fn draw_centered_text(&mut self, text: &str, position: Vector2, font_size: f32, color: Color) {
        let snitched_from_source_code = 10.0;
        let size = measure_text_ex(
            self.get_font_default(),
            text,
            font_size,
            font_size / snitched_from_source_code,
        );
        self.draw_text(
            text,
            (position.x - size.x / 2.0) as i32,
            (position.y - size.y / 2.0) as i32,
            font_size as i32,
            color,
        );
    }
}

pub trait ImageExtension {
    fn bounds(&self) -> Rectangle;
}

impl ImageExtension for Image {
    fn bounds(&self) -> Rectangle {
        Rectangle::new(0.0, 0.0, self.width() as f32, self.height() as f32)
    }
}

pub trait Vector2Extension {
    fn rad(dir: f32, len: f32) -> Self;
}

impl Vector2Extension for Vector2 {
    #[inline]
    fn rad(dir: f32, len: f32) -> Self {
        let (s, c) = dir.sin_cos();
        Self::new(c * len, s * len)
    }
}

pub trait RectangleExtension {
    fn square(pos: Vector2, radius: f32) -> Self;

    fn center(&self) -> Vector2;

    fn top(&self) -> f32;
    fn right(&self) -> f32;

    fn fits_in(&self, other: &Self) -> bool;
}

impl RectangleExtension for Rectangle {
    #[inline]
    fn square(pos: Vector2, radius: f32) -> Self {
        Self::new(pos.x - radius, pos.y - radius, radius * 2f32, radius * 2f32)
    }

    #[inline]
    fn center(&self) -> Vector2 {
        Vector2::new(self.x + self.width * 0.5, self.y + self.height * 0.5)
    }

    #[inline]
    fn top(&self) -> f32 {
        self.y + self.height
    }

    #[inline]
    fn right(&self) -> f32 {
        self.x + self.width
    }

    #[inline]
    fn fits_in(&self, other: &Self) -> bool {
        self.x >= other.x
            && self.y >= other.y
            && self.top() <= other.top()
            && self.right() <= other.right()
    }
}

pub fn bench(name: &str, f: impl FnOnce()) {
    let start = std::time::Instant::now();
    f();
    let end = start.elapsed();
    println!("[{}] {}s", name, end.as_secs_f64());
}