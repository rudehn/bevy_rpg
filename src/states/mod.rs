use bevy::prelude::*;
use bevy_light_2d::plugin::Light2dPlugin;

use crate::states::game::GamePlugin;
use crate::states::loading::LoadingPlugin;

mod game;
mod loading;

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    Loading,
    InGame,
}

pub struct StatePlugin;
impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>();
        app.add_plugins((Light2dPlugin, LoadingPlugin, GamePlugin));
    }
}
