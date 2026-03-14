use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bracket_lib::prelude::{Algorithm2D, Point};

use crate::{
    components::{GameEntityMarker, Position, Viewshed},
    game::{
        AppState,
        combat::{ApplyDamageMessage, CombatDamageSet, HealMessage, MissMessage},
    },
    map::{tile::is_opaque, Map},
    player::Player,
};

// --- Constants ---

const Z_PARTICLE: f32 = 10.0;
const LIFETIME_FLOAT: f32 = 1.2;
const LIFETIME_IMPACT: f32 = 0.5;
const FLOAT_VEL: Vec2 = Vec2::new(0.0, 20.0);

// --- Resources ---

#[derive(Resource)]
pub struct ParticleFont(pub Handle<Font>);

// --- Public API ---

#[derive(Clone, Debug)]
pub struct ImpactData {
    pub glyph: char,
    pub color: Color,
    pub font_size: f32,
    pub duration: f32,
}

#[derive(Message, Clone, Debug)]
pub enum ParticleRequest {
    /// Floating text that rises and fades (damage numbers, heals, misses).
    FloatingText {
        world_pos: Vec2,
        text: String,
        color: Color,
        font_size: f32,
    },
    /// A glyph that travels from source to destination; optionally spawns an
    /// impact on arrival (spell hit, arrow landing).
    Projectile {
        source: Vec2,
        destination: Vec2,
        glyph: char,
        color: Color,
        speed: f32, // world-units per second
        on_impact: Option<ImpactData>,
    },
    /// Brief flash at a position; no movement, just fades.
    Impact {
        world_pos: Vec2,
        glyph: char,
        color: Color,
        font_size: f32,
        duration: f32,
    },
    /// AoE flash filtered by wall LOS and player FOV.
    AoeImpact {
        center_grid: IVec2,
        radius: u32,
        glyph: char,
        color: Color,
        font_size: f32,
        duration: f32,
    },
}

impl ParticleRequest {
    /// Red floating damage number (e.g., "−12"), offset to the right side of the tile.
    pub fn damage(world_pos: Vec2, amount: i32) -> Self {
        Self::FloatingText {
            world_pos: Vec2::new(world_pos.x + 10.0, world_pos.y),
            text: format!("\u{2212}{}", amount), // Unicode minus sign
            color: Color::srgb(1.0, 0.2, 0.2),
            font_size: 4.0,
        }
    }

    /// Green floating heal number (e.g., "+5"), offset to the right side of the tile.
    pub fn heal(world_pos: Vec2, amount: i32) -> Self {
        Self::FloatingText {
            world_pos: Vec2::new(world_pos.x + 10.0, world_pos.y),
            text: format!("+{}", amount),
            color: Color::srgb(0.2, 1.0, 0.2),
            font_size: 3.5,
        }
    }

    /// Gray floating "miss" text, offset to the right side of the tile.
    pub fn miss(world_pos: Vec2) -> Self {
        Self::FloatingText {
            world_pos: Vec2::new(world_pos.x + 10.0, world_pos.y),
            text: "miss".to_string(),
            color: Color::srgb(0.6, 0.6, 0.6),
            font_size: 3.0,
        }
    }

    /// Magic '*' projectile that spawns an impact glyph on arrival.
    pub fn spell(src: Vec2, dst: Vec2, color: Color) -> Self {
        Self::Projectile {
            source: src,
            destination: dst,
            glyph: '*',
            color,
            speed: 80.0,
            on_impact: Some(ImpactData {
                glyph: '*',
                color,
                font_size: 18.0,
                duration: LIFETIME_IMPACT,
            }),
        }
    }

    /// Tan '·' arrow projectile with no impact flash.
    pub fn arrow(src: Vec2, dst: Vec2) -> Self {
        Self::Projectile {
            source: src,
            destination: dst,
            glyph: '\u{00B7}', // ·
            color: Color::srgb(0.82, 0.71, 0.55),
            speed: 120.0,
            on_impact: None,
        }
    }

    /// Single '*' impact flash at the given position.
    pub fn impact(world_pos: Vec2, color: Color) -> Self {
        Self::Impact {
            world_pos,
            glyph: '*',
            color,
            font_size: 18.0,
            duration: LIFETIME_IMPACT,
        }
    }
}

// --- Components ---

#[derive(Component)]
pub struct ParticleEffect {
    pub lifetime_remaining: f32,
    pub lifetime_total: f32,
}

#[derive(Component)]
pub enum ParticleKind {
    Float { velocity: Vec2 },
    Projectile { destination: Vec2, speed: f32, on_impact: Option<ImpactData> },
    Impact,
}

// --- Setup ---

fn setup_particle_font(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(ParticleFont(
        asset_server.load("fonts/Macondo-Regular.ttf"),
    ));
}

// --- Helpers ---

pub fn grid_to_world(gx: i32, gy: i32) -> Vec2 {
    use crate::map::map::GRID_SIZE;
    Vec2::new(gx as f32 * GRID_SIZE.x, gy as f32 * GRID_SIZE.y + GRID_SIZE.y * 0.5)
}

fn spawn_impact_entity(commands: &mut Commands, font: &ParticleFont, world_pos: Vec2, impact: &ImpactData) {
    commands.spawn((
        Text2d::new(impact.glyph.to_string()),
        TextFont {
            font: font.0.clone(),
            font_size: impact.font_size,
            ..default()
        },
        TextColor(impact.color),
        Transform::from_translation(world_pos.extend(Z_PARTICLE)),
        RenderLayers::layer(1),
        GameEntityMarker,
        ParticleEffect {
            lifetime_remaining: impact.duration,
            lifetime_total: impact.duration,
        },
        ParticleKind::Impact,
    ));
}

/// Simple Bresenham line walk from start to end (inclusive).
fn bresenham(start: (i32, i32), end: (i32, i32)) -> Vec<(i32, i32)> {
    let (mut x, mut y) = start;
    let (x1, y1) = end;
    let dx = (x1 - x).abs();
    let dy = -(y1 - y).abs();
    let sx = if x < x1 { 1 } else { -1 };
    let sy = if y < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut pts = Vec::new();
    loop {
        pts.push((x, y));
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
    pts
}

// --- Systems ---

pub fn particle_spawn_system(
    mut requests: MessageReader<ParticleRequest>,
    mut commands: Commands,
    font: Res<ParticleFont>,
    map: Option<Res<Map>>,
    player_viewshed: Query<&Viewshed, With<Player>>,
) {
    for req in requests.read() {
        match req {
            ParticleRequest::FloatingText { world_pos, text, color, font_size } => {
                commands.spawn((
                    Text2d::new(text.clone()),
                    TextFont {
                        font: font.0.clone(),
                        font_size: *font_size,
                        ..default()
                    },
                    TextColor(*color),
                    Transform::from_translation(world_pos.extend(Z_PARTICLE)),
                    RenderLayers::layer(1),
                    GameEntityMarker,
                    ParticleEffect {
                        lifetime_remaining: LIFETIME_FLOAT,
                        lifetime_total: LIFETIME_FLOAT,
                    },
                    ParticleKind::Float { velocity: FLOAT_VEL },
                ));
            }

            ParticleRequest::Projectile { source, destination, glyph, color, speed, on_impact } => {
                let dist = source.distance(*destination);
                let travel_duration = if *speed > 0.0 { dist / speed } else { 0.1 };
                commands.spawn((
                    Text2d::new(glyph.to_string()),
                    TextFont {
                        font: font.0.clone(),
                        font_size: 16.0,
                        ..default()
                    },
                    TextColor(*color),
                    Transform::from_translation(source.extend(Z_PARTICLE)),
                    RenderLayers::layer(1),
                    GameEntityMarker,
                    ParticleEffect {
                        lifetime_remaining: travel_duration,
                        lifetime_total: travel_duration,
                    },
                    ParticleKind::Projectile {
                        destination: *destination,
                        speed: *speed,
                        on_impact: on_impact.clone(),
                    },
                ));
            }

            ParticleRequest::Impact { world_pos, glyph, color, font_size, duration } => {
                commands.spawn((
                    Text2d::new(glyph.to_string()),
                    TextFont {
                        font: font.0.clone(),
                        font_size: *font_size,
                        ..default()
                    },
                    TextColor(*color),
                    Transform::from_translation(world_pos.extend(Z_PARTICLE)),
                    RenderLayers::layer(1),
                    GameEntityMarker,
                    ParticleEffect {
                        lifetime_remaining: *duration,
                        lifetime_total: *duration,
                    },
                    ParticleKind::Impact,
                ));
            }

            ParticleRequest::AoeImpact { center_grid, radius, glyph, color, font_size, duration } => {
                let Some(map) = &map else { continue; };
                let viewshed = player_viewshed.single().ok();
                let r = *radius as i32;
                let cx = center_grid.x;
                let cy = center_grid.y;

                for ty in (cy - r)..=(cy + r) {
                    for tx in (cx - r)..=(cx + r) {
                        // Bounds check
                        if !map.in_bounds(Point::new(tx, ty)) {
                            continue;
                        }

                        // Wall LOS check: walk from center to each tile, stop at opaque
                        let mut blocked = false;
                        for (px, py) in bresenham((cx, cy), (tx, ty)) {
                            if !map.in_bounds(Point::new(px, py)) {
                                blocked = true;
                                break;
                            }
                            let idx = map.xy_idx(px, py);
                            if is_opaque(map.tiles[idx]) {
                                // Wall tile itself gets a particle; tiles behind it do not
                                if px != tx || py != ty {
                                    blocked = true;
                                }
                                break;
                            }
                        }
                        if blocked {
                            continue;
                        }

                        // FOV check: only render if within the player's visible tiles
                        if let Some(vs) = viewshed {
                            if !vs.visible_tiles.contains(&Point::new(tx, ty)) {
                                continue;
                            }
                        }

                        let world_pos = grid_to_world(tx, ty);
                        commands.spawn((
                            Text2d::new(glyph.to_string()),
                            TextFont {
                                font: font.0.clone(),
                                font_size: *font_size,
                                ..default()
                            },
                            TextColor(*color),
                            Transform::from_translation(world_pos.extend(Z_PARTICLE)),
                            RenderLayers::layer(1),
                            GameEntityMarker,
                            ParticleEffect {
                                lifetime_remaining: *duration,
                                lifetime_total: *duration,
                            },
                            ParticleKind::Impact,
                        ));
                    }
                }
            }
        }
    }
}

pub fn particle_update_system(
    mut commands: Commands,
    time: Res<Time>,
    font: Res<ParticleFont>,
    mut particles: Query<(Entity, &mut Transform, &mut TextColor, &mut ParticleEffect, &mut ParticleKind)>,
) {
    let dt = time.delta_secs();
    for (entity, mut transform, mut text_color, mut effect, mut kind) in particles.iter_mut() {
        effect.lifetime_remaining -= dt;
        if effect.lifetime_remaining <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }

        let alpha = (effect.lifetime_remaining / effect.lifetime_total).clamp(0.0, 1.0);
        text_color.0 = text_color.0.with_alpha(alpha);

        match &mut *kind {
            ParticleKind::Float { velocity } => {
                transform.translation += velocity.extend(0.0) * dt;
            }
            ParticleKind::Projectile { destination, speed, on_impact } => {
                let current_pos = transform.translation.xy();
                let remaining = *destination - current_pos;
                let dir = remaining.normalize_or_zero();
                let step = dir * *speed * dt;

                if step.length() >= remaining.length() {
                    // Arrived — spawn impact glyph and despawn the projectile
                    if let Some(impact) = on_impact.take() {
                        spawn_impact_entity(&mut commands, &font, *destination, &impact);
                    }
                    commands.entity(entity).despawn();
                } else {
                    transform.translation += step.extend(0.0);
                }
            }
            ParticleKind::Impact => {
                // Alpha fade handled above; no positional update needed
            }
        }
    }
}

/// Translates combat messages into ParticleRequests for visual feedback.
pub fn combat_particle_bridge_system(
    mut damage_messages: MessageReader<ApplyDamageMessage>,
    mut heal_messages: MessageReader<HealMessage>,
    mut miss_messages: MessageReader<MissMessage>,
    mut particle_writer: MessageWriter<ParticleRequest>,
    pos_query: Query<&Position>,
) {
    for msg in damage_messages.read() {
        if let Ok(pos) = pos_query.get(msg.target) {
            particle_writer.write(ParticleRequest::damage(grid_to_world(pos.x, pos.y), msg.final_damage));
        }
    }
    for msg in heal_messages.read() {
        if let Ok(pos) = pos_query.get(msg.entity) {
            particle_writer.write(ParticleRequest::heal(grid_to_world(pos.x, pos.y), msg.amount));
        }
    }
    for msg in miss_messages.read() {
        if let Ok(pos) = pos_query.get(msg.target) {
            particle_writer.write(ParticleRequest::miss(grid_to_world(pos.x, pos.y)));
        }
    }
}

// --- Plugin ---

pub struct ParticlesPlugin;

impl Plugin for ParticlesPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ParticleRequest>()
            .add_systems(OnEnter(AppState::InGame), setup_particle_font)
            .add_systems(
                Update,
                (
                    combat_particle_bridge_system.after(CombatDamageSet),
                    particle_spawn_system.after(combat_particle_bridge_system),
                    particle_update_system,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}
