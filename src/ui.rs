use std::ffi::CStr;

use util::prelude::*;

pub enum State {
    MainMenu,
    PlayMenu,
    MainOptions,
}

pub fn main_menu(handle: &mut RaylibDrawHandle) -> MainMenuAction {
    let bounds = handle.get_screen_rect();
    let center = bounds.center();

    let title = "DefOut";
    handle.draw_centered_text(title, center, 50.0, Color::BLACK);

    default_button_layout(
        handle,
        &[
            (util::cstr!("Play"), MainMenuAction::Play),
            (util::cstr!("Options"), MainMenuAction::Options),
            (util::cstr!("Quit"), MainMenuAction::Quit),
        ],
        MainMenuAction::None,
    )
}

#[derive(Debug, Copy, Clone)]
pub enum MainMenuAction {
    Play,
    Options,
    Quit,
    None,
}

pub fn play_menu(handle: &mut RaylibDrawHandle) -> PlayMenuAction {
    let bounds = handle.get_screen_rect();
    let center = bounds.center();

    let title = "Choose a game mode!";

    handle.draw_centered_text(title, center, 30.0, Color::BLACK);

    default_button_layout(
        handle,
        &[
            (util::cstr!("SinglePlayer"), PlayMenuAction::SinglePlayer),
            (util::cstr!("MultiPlayer"), PlayMenuAction::MultiPlayer),
            (util::cstr!("Back"), PlayMenuAction::Back),
        ],
        PlayMenuAction::None,
    )
}

#[derive(Debug, Clone, Copy)]
pub enum PlayMenuAction {
    SinglePlayer,
    MultiPlayer,
    Back,
    None,
}

pub struct MainOptions {
    pub fps_changed: bool,
    pub editing_fps: bool,
    pub fps_limit: i32,
}

impl MainOptions {
    pub fn new() -> Self {
        Self {
            fps_changed: true,
            editing_fps: false,
            fps_limit: 60,
        }
    }

    pub fn update(&mut self, handle: &mut RaylibHandle) {
        if self.fps_changed {
            handle.set_target_fps(self.fps_limit as u32);
            self.fps_changed = false;
        }
    }

    pub fn draw(&mut self, handle: &mut RaylibDrawHandle) -> MainOptionsAction {
        let bounds = handle.get_screen_rect();
        let center = bounds.center();

        let title = "Options";
        handle.draw_centered_text(title, center, 30.0, Color::BLACK);

        // fps spinner
        let old = self.fps_limit;
        if handle.gui_spinner(
            rrect(70, 20, 100, 25),
            Some(util::cstr!("Max Fps")),
            &mut self.fps_limit,
            20,
            240,
            self.editing_fps,
        ) {
            self.fps_changed = true;
            self.editing_fps = !self.editing_fps;
        } else {
            self.fps_changed = old != self.fps_limit && !self.editing_fps;
        }

        // bottom buttons
        horizontal_button_layout(
            handle,
            Vector2::new(center.x, bounds.height * 0.8),
            bounds.width / 9.0,
            bounds.height / 9.0,
            bounds.width / 20.0,
            &[(util::cstr!("Back"), MainOptionsAction::Back)],
            MainOptionsAction::None,
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MainOptionsAction {
    Back,
    None,
}

pub fn default_button_layout<T: Copy>(
    handle: &mut RaylibDrawHandle,
    data: &[(&CStr, T)],
    default_state: T,
) -> T {
    let bounds = handle.get_screen_rect();
    let center = bounds.center();

    circle_button_layout(
        handle,
        center,
        bounds.width * 0.35,
        bounds.height * 0.35,
        bounds.width / 9.0,
        bounds.height / 9.0,
        data,
        default_state,
    )
}

pub fn horizontal_button_layout<T: Copy>(
    handle: &mut RaylibDrawHandle,
    center: Vector2,
    button_width: f32,
    button_height: f32,
    spacing: f32,
    data: &[(&CStr, T)],
    mut default_state: T,
) -> T {
    let total_width = button_width * data.len() as f32 + spacing * (data.len() as f32 - 1.0);
    let y = center.y - button_height / 2.0;
    let mut x = center.x - total_width / 2.0;
    for (text, state) in data {
        if handle.gui_button(
            Rectangle::new(x, y, button_width, button_height),
            Some(text),
        ) {
            default_state = *state;
        }
        x += button_width + spacing;
    }

    default_state
}

pub fn circle_button_layout<T: Copy>(
    handle: &mut RaylibDrawHandle,
    center: Vector2,
    circle_width: f32,
    circle_height: f32,
    button_width: f32,
    button_height: f32,
    data: &[(&CStr, T)],
    mut default_state: T,
) -> T {
    let angle_step = 2.0 * std::f32::consts::PI / data.len() as f32;
    let angle_origin = std::f32::consts::PI / 2.0;
    for (i, &(name, state)) in data.iter().enumerate() {
        let angle = angle_origin + angle_step * i as f32;
        let position =
            center + Vector2::new(angle.cos() * circle_width, angle.sin() * circle_height);
        if handle.gui_button(
            Rectangle::new(
                position.x - button_width / 2.0,
                position.y - button_height / 2.0,
                button_width,
                button_height,
            ),
            Some(name),
        ) {
            default_state = state;
        }
    }

    default_state
}
