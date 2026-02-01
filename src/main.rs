use bevy::prelude::*;

mod assets_plugin;
mod components;
mod constants;
mod map;
mod player;
mod states;

use crate::states::StatePlugin;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(ImagePlugin::default_nearest()),
            StatePlugin,
        ))
        .run();
}
