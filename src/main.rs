use bevy::prelude::*;

use crate::assets::LoadingPlugin;
use crate::game::{AppState, GamePlugin};
use crate::menu::MenuPlugin; // Import the new MenuPlugin

mod assets;
mod components;
mod constants;
mod game;
mod map;
mod menu;
mod player; // Declare the new menu module

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(ImagePlugin::default_nearest()),
            (LoadingPlugin, GamePlugin, MenuPlugin), // Add MenuPlugin here
        ))
        .init_state::<AppState>()
        .run();
}
