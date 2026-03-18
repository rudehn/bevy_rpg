use bevy::prelude::*;

use crate::map::tile::TileMarker;

/// Controls whether the game renders sprites or ASCII characters.
#[derive(Resource, Default, PartialEq, Eq, Clone, Copy)]
pub enum GraphicsMode {
    #[default]
    Sprites,
    Ascii,
}

/// Marker for the solid-color background quad on tile entities (ASCII mode).
#[derive(Component)]
pub struct AsciiBackground;

/// Marker for the Text2d character glyph on any entity (ASCII mode).
#[derive(Component)]
pub struct AsciiGlyph;

/// Marker for liquid overlay sprite children on tile entities.
#[derive(Component)]
pub struct LiquidOverlay;

/// Monospace font handle for ASCII glyphs.
#[derive(Resource)]
pub struct AsciiFont(pub Handle<Font>);

pub struct AsciiModePlugin;

impl Plugin for AsciiModePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GraphicsMode>()
            .add_systems(Startup, load_ascii_font)
            .add_systems(
                Update,
                (toggle_graphics_mode, apply_graphics_mode_swap)
                    .run_if(in_state(crate::game::AppState::InGame)),
            );
    }
}

fn load_ascii_font(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/SourceCodePro.ttf");
    commands.insert_resource(AsciiFont(font));
}

/// F5 toggles between Sprites and ASCII mode.
fn toggle_graphics_mode(keys: Res<ButtonInput<KeyCode>>, mut mode: ResMut<GraphicsMode>) {
    if keys.just_pressed(KeyCode::Equal) {
        *mode = match *mode {
            GraphicsMode::Sprites => GraphicsMode::Ascii,
            GraphicsMode::Ascii => GraphicsMode::Sprites,
        };
    }
}

/// When GraphicsMode changes, swap visibility of sprite vs ASCII children.
fn apply_graphics_mode_swap(
    mode: Res<GraphicsMode>,
    mut clear_color: ResMut<ClearColor>,
    mut tile_sprites: Query<&mut Sprite, With<TileMarker>>,
    mut ascii_bgs: Query<
        &mut Visibility,
        (
            With<AsciiBackground>,
            Without<AsciiGlyph>,
            Without<LiquidOverlay>,
        ),
    >,
    mut ascii_glyphs: Query<
        &mut Visibility,
        (
            With<AsciiGlyph>,
            Without<AsciiBackground>,
            Without<LiquidOverlay>,
        ),
    >,
    mut liquids: Query<
        &mut Visibility,
        (
            With<LiquidOverlay>,
            Without<AsciiBackground>,
            Without<AsciiGlyph>,
        ),
    >,
    mut entity_sprites: Query<&mut Sprite, (Without<TileMarker>, Without<AsciiBackground>)>,
) {
    if !mode.is_changed() {
        return;
    }

    let is_ascii = *mode == GraphicsMode::Ascii;

    // Set clear color to black in ASCII mode, restore original in Sprites mode
    clear_color.0 = if is_ascii {
        Color::BLACK
    } else {
        Color::srgb_u8(37, 19, 26)
    };

    for mut sprite in tile_sprites.iter_mut() {
        sprite.color = if is_ascii { Color::NONE } else { Color::WHITE };
    }

    for mut vis in ascii_bgs.iter_mut() {
        *vis = if is_ascii {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    for mut vis in ascii_glyphs.iter_mut() {
        *vis = if is_ascii {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    for mut vis in liquids.iter_mut() {
        *vis = if is_ascii {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }

    for mut sprite in entity_sprites.iter_mut() {
        sprite.color = if is_ascii { Color::NONE } else { Color::WHITE };
    }
}
