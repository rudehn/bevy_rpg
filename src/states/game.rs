use bevy::prelude::*;

use crate::{
    map::{light::LightPlugin, map::MapPlugin},
    player::player::PlayerPlugin,
};

pub struct GamePlugin;
impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((LightPlugin, MapPlugin, PlayerPlugin));
    }
}
