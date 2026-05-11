use bevy::prelude::*;

use crate::map::tile::TileMarker;
use bevy::ecs::hierarchy::ChildOf;

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

/// Stores the ASCII character and color for an entity, used by the cell renderer
/// to display entities on tile glyphs. Added directly to the entity (not a child).
#[derive(Component, Clone)]
pub struct AsciiDisplay {
    pub ch: String,
    pub color: Color,
}

/// Monospace font handle for ASCII glyphs.
#[derive(Resource)]
pub struct AsciiFont(pub Handle<Font>);

pub struct AsciiModePlugin;

impl Plugin for AsciiModePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_ascii_font)
            .add_systems(
                Update,
                init_new_ascii_glyphs
                    .run_if(in_state(crate::game::AppState::InGame)),
            );
    }
}

fn load_ascii_font(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/SourceCodePro.ttf");
    commands.insert_resource(AsciiFont(font));
}

/// Set the correct visibility on newly spawned AsciiGlyph and AsciiBackground
/// entities. Tile glyphs (children of TileMarker) and player glyphs are
/// Inherited (visible), but entity glyphs (children of monsters/items/props)
/// are Hidden because the unified tile renderer draws entity glyphs onto tile
/// children.
fn init_new_ascii_glyphs(
    mut new_glyphs: Query<(&mut Visibility, &ChildOf), (Added<AsciiGlyph>, Without<AsciiBackground>)>,
    mut new_bgs: Query<&mut Visibility, (Added<AsciiBackground>, Without<AsciiGlyph>)>,
    tile_markers: Query<(), With<TileMarker>>,
) {
    for (mut vis, child_of) in new_glyphs.iter_mut() {
        let is_tile_glyph = tile_markers.get(child_of.0).is_ok();
        *vis = if is_tile_glyph {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    for mut vis in new_bgs.iter_mut() {
        *vis = Visibility::Inherited;
    }
}
