use bevy::prelude::*;
use bevy_light_2d::plugin::Light2dPlugin;

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
