use bevy::prelude::*;

use crate::{
    constants::{TILE_MAP_PATH, TILE_SIZE_X, TILE_SIZE_Y},
    map::light::CandleSpritesheet,
    states::AppState,
};

#[derive(Resource, Default)]
pub struct DungeonTileset {
    pub layout: Handle<TextureAtlasLayout>,
    pub texture: Handle<Image>,
}

fn setup_dungeon_tileset(
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut dungeon_tileset: ResMut<DungeonTileset>,
) {
    dungeon_tileset.texture = asset_server.load(TILE_MAP_PATH);
    dungeon_tileset.layout = texture_atlas_layouts.add(TextureAtlasLayout::from_grid(
        UVec2::new(16, 16),
        12,
        11,
        None,
        None,
    ));
}

pub struct AssetsPlugin;

impl Plugin for AssetsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DungeonTileset>().add_systems(
            OnEnter(AppState::Loading),
            (setup_dungeon_tileset, setup_candle_spritesheet),
        );
    }
}

fn setup_candle_spritesheet(
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut candle_spritesheet: ResMut<CandleSpritesheet>,
) {
    candle_spritesheet.texture = asset_server.load("candle.png");
    candle_spritesheet.layout = texture_atlas_layouts.add(TextureAtlasLayout::from_grid(
        UVec2::new(TILE_SIZE_X, TILE_SIZE_Y),
        4,
        1,
        None,
        None,
    ));
}
