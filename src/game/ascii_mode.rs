use bevy::prelude::*;

use crate::map::tile::TileMarker;

/// Controls whether the game renders sprites or ASCII characters.
#[derive(Resource, Default, PartialEq, Eq, Clone, Copy)]
pub enum GraphicsMode {
    Sprites,
    #[default]
    Ascii,
}

/// Marker for the solid-color background quad on tile entities (ASCII mode).
/// Stores the original baked color so visibility tinting can multiply against it.
#[derive(Component)]
pub struct AsciiBackground {
    pub base_color: Color,
}

/// Stores the original foreground color for an ASCII glyph.
#[derive(Component)]
pub struct AsciiGlyphColor(pub Color);

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
                (toggle_graphics_mode, apply_graphics_mode_swap, init_new_ascii_glyphs, update_player_ascii_sprite)
                    .run_if(in_state(crate::game::AppState::InGame)),
            );
    }
}

fn load_ascii_font(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/SourceCodePro.ttf");
    commands.insert_resource(AsciiFont(font));
}

/// F5 toggles between Sprites and ASCII mode.
/// Also marks the player viewshed dirty so visibility systems re-run immediately.
fn toggle_graphics_mode(
    keys: Res<ButtonInput<KeyCode>>,
    mut mode: ResMut<GraphicsMode>,
    mut viewshed_query: Query<&mut crate::components::Viewshed, With<crate::player::Player>>,
) {
    if keys.just_pressed(KeyCode::Equal) {
        *mode = match *mode {
            GraphicsMode::Sprites => GraphicsMode::Ascii,
            GraphicsMode::Ascii => GraphicsMode::Sprites,
        };
        // Force visibility systems to re-evaluate this frame
        if let Ok(mut vs) = viewshed_query.single_mut() {
            vs.dirty = true;
        }
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

    // Tile sprites: transparent in ASCII mode (visibility systems handle per-frame color)
    for mut sprite in tile_sprites.iter_mut() {
        sprite.color = if is_ascii { Color::NONE } else { Color::WHITE };
    }

    // ASCII backgrounds: visible in ASCII mode
    for mut vis in ascii_bgs.iter_mut() {
        *vis = if is_ascii {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    // ASCII glyphs: visible in ASCII mode
    for mut vis in ascii_glyphs.iter_mut() {
        *vis = if is_ascii {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    // Liquid overlays: hidden in ASCII mode
    for mut vis in liquids.iter_mut() {
        *vis = if is_ascii {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }

    // Note: monster, item, and prop sprites are handled by their visibility systems
    // which branch on GraphicsMode. Player sprite is handled by update_player_ascii_sprite.
}

/// Set the correct visibility on newly spawned AsciiGlyph, AsciiBackground,
/// and LiquidOverlay entities based on the current graphics mode. This catches
/// entities created after the last mode toggle (e.g., floor transitions, items
/// spawned from chests at runtime).
fn init_new_ascii_glyphs(
    mode: Res<GraphicsMode>,
    mut new_glyphs: Query<&mut Visibility, (Added<AsciiGlyph>, Without<AsciiBackground>, Without<LiquidOverlay>)>,
    mut new_bgs: Query<&mut Visibility, (Added<AsciiBackground>, Without<AsciiGlyph>, Without<LiquidOverlay>)>,
    mut new_liquids: Query<&mut Visibility, (Added<LiquidOverlay>, Without<AsciiGlyph>, Without<AsciiBackground>)>,
) {
    let is_ascii = *mode == GraphicsMode::Ascii;
    let glyph_vis = if is_ascii { Visibility::Inherited } else { Visibility::Hidden };
    let bg_vis = if is_ascii { Visibility::Inherited } else { Visibility::Hidden };
    let liquid_vis = if is_ascii { Visibility::Hidden } else { Visibility::Inherited };

    for mut vis in new_glyphs.iter_mut() {
        *vis = glyph_vis;
    }
    for mut vis in new_bgs.iter_mut() {
        *vis = bg_vis;
    }
    for mut vis in new_liquids.iter_mut() {
        *vis = liquid_vis;
    }
}

/// Keep the player sprite transparent in ASCII mode, restore in Sprites mode.
/// Also handles newly spawned players so the sprite is correct on first frame.
fn update_player_ascii_sprite(
    mode: Res<GraphicsMode>,
    mut params: ParamSet<(
        Query<&mut Sprite, With<crate::player::Player>>,
        Query<(), (With<crate::player::Player>, Added<Sprite>)>,
    )>,
) {
    let has_new = !params.p1().is_empty();
    if !mode.is_changed() && !has_new {
        return;
    }
    let is_ascii = *mode == GraphicsMode::Ascii;
    for mut sprite in params.p0().iter_mut() {
        sprite.color = if is_ascii { Color::NONE } else { Color::WHITE };
    }
}
