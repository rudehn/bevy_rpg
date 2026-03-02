use bevy::prelude::*;

use crate::assets::LoadingPlugin;
use crate::game::{AppState, GamePlugin};
use crate::menu::MenuPlugin;
use crate::ui::UiPlugin; // Added UiPlugin import

mod assets;
mod components;
mod constants;
mod game;
mod map;
mod menu;
mod player;
mod ui; // Added ui module declaration

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins
                .set(ImagePlugin::default_nearest())
                .set(AssetPlugin {
                    watch_for_changes_override: Some(true),
                    ..default()
                }),
            (LoadingPlugin, GamePlugin, MenuPlugin, UiPlugin),
        ))
        .init_state::<AppState>()
        .run();
}
