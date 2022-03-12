use ui::MainOptions;
use util::prelude::*;
use bitwise::*;

mod components;
mod ui;

#[derive(Bitwise)]
pub struct Smh {
    pub foo: Vec<bool>,
}

fn main() {
    let (mut rl, thread) = raylib::init().resizable().title("Hello, World").build();

    let mut ui_state = ui::State::MainMenu;
    let mut main_options = MainOptions::new();

    while !rl.window_should_close() {
        let mut d = rl.begin_drawing(&thread);

        d.clear_background(Color::RAYWHITE);

        match ui_state {
            ui::State::MainMenu => match ui::main_menu(&mut d) {
                ui::MainMenuAction::Play => ui_state = ui::State::PlayMenu,
                ui::MainMenuAction::Options => ui_state = ui::State::MainOptions,
                ui::MainMenuAction::Quit => break,
                ui::MainMenuAction::None => (),
            },
            ui::State::PlayMenu => match ui::play_menu(&mut d) {
                ui::PlayMenuAction::SinglePlayer => todo!(),
                ui::PlayMenuAction::MultiPlayer => todo!(),
                ui::PlayMenuAction::Back => ui_state = ui::State::MainMenu,
                ui::PlayMenuAction::None => (),
            },
            ui::State::MainOptions => match main_options.draw(&mut d) {
                ui::MainOptionsAction::Back => ui_state = ui::State::MainMenu,
                ui::MainOptionsAction::None => (),
            },
        }

        drop(d);

        main_options.update(&mut rl);
    }
}
