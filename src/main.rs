use bevy::asset::AssetMetaCheck;
use bevy::diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};
use bevy::prelude::*;

use crate::assets::LoadingPlugin;
use crate::game::{AppState, GamePlugin};
use crate::save::SavePlugin;
use crate::ui::UiPlugin;

mod assets;
mod components;
mod constants;
mod game;
mod map;
mod player;
mod save;
mod ui;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins
                .set(ImagePlugin::default_nearest())
                .set(AssetPlugin {
                    meta_check: AssetMetaCheck::Never,
                    ..default()
                }),
            (LoadingPlugin, SavePlugin, GamePlugin, UiPlugin),
            (FrameTimeDiagnosticsPlugin::default(), LogDiagnosticsPlugin::default()),
        ))
        .init_state::<AppState>()
        .run();
}
