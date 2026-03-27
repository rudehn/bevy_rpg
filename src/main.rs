// Bevy systems inherently have many parameters and complex query types.
// Enum variant naming uses SCREAMING_CASE for legacy bracket-lib compatibility.
#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::upper_case_acronyms
)]

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
            FrameTimeDiagnosticsPlugin::default(),
        ))
        .init_state::<AppState>()
        .run();
}
