use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bracket_lib::prelude::{Algorithm2D, Point};

use crate::{
    components::{GameEntityMarker, Position, Viewshed},
    game::{
        AppState,
        combat::{CombatDamageSet, CombatEventSet, DamageEvent, HealEvent, MissMessage},
    },
    map::{Map, tile::is_opaque},
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
    /// A sprite-based projectile (arrow, lightning, etc.) that travels from
    /// source to destination using an image asset instead of a text glyph.
    SpriteProjectile {
        source: Vec2,
        destination: Vec2,
        sprite_path: String,
        speed: f32,
        on_impact: Option<ImpactData>,
    },
    /// An animated sprite projectile that cycles through multiple frames
    /// while traveling from source to destination (fire bolts, lightning).
    AnimatedSpriteProjectile {
        source: Vec2,
        destination: Vec2,
        sprite_paths: Vec<String>,
        frame_rate: f32,
        speed: f32,
        on_impact: Option<ImpactData>,
    },
    /// Static animated sprite flash at a position (fire burst, lightning impact).
    SpriteImpact {
        world_pos: Vec2,
        sprite_paths: Vec<String>,
        frame_rate: f32,
        duration: f32,
    },
    /// Brief flash at a position; no movement, just fades.
    #[allow(dead_code)]
    Impact {
        world_pos: Vec2,
        glyph: char,
        color: Color,
        font_size: f32,
        duration: f32,
    },
    /// AoE flash filtered by wall LOS and player FOV.
    #[allow(dead_code)]
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

    /// Sprite-based arrow projectile with direction-appropriate sprite.
    pub fn arrow(src_grid: (i32, i32), dst_grid: (i32, i32)) -> Self {
        let src = grid_to_world_center(src_grid.0, src_grid.1);
        let dst = grid_to_world_center(dst_grid.0, dst_grid.1);
        let dir_idx = direction_index_8(src, dst);
        Self::SpriteProjectile {
            source: src,
            destination: dst,
            sprite_path: format!("sprites/effects/arrow_{}.png", dir_idx),
            speed: 120.0,
            on_impact: None,
        }
    }

    /// Animated lightning projectile — cycles through zap_0..3 sprites rapidly.
    pub fn lightning(src_grid: (i32, i32), dst_grid: (i32, i32)) -> Self {
        let src = grid_to_world_center(src_grid.0, src_grid.1);
        let dst = grid_to_world_center(dst_grid.0, dst_grid.1);
        Self::AnimatedSpriteProjectile {
            source: src,
            destination: dst,
            sprite_paths: (0..4)
                .map(|i| format!("sprites/effects/zap_{}.png", i))
                .collect(),
            frame_rate: 500.0,
            speed: 160.0,
            on_impact: None,
        }
    }

    /// Animated fire projectile — cycles through flame_0..2 with orange impact on arrival.
    pub fn fire_bolt(src_grid: (i32, i32), dst_grid: (i32, i32)) -> Self {
        let src = grid_to_world_center(src_grid.0, src_grid.1);
        let dst = grid_to_world_center(dst_grid.0, dst_grid.1);
        Self::AnimatedSpriteProjectile {
            source: src,
            destination: dst,
            sprite_paths: (0..3)
                .map(|i| format!("sprites/effects/flame_{}.png", i))
                .collect(),
            frame_rate: 10.0,
            speed: 100.0,
            on_impact: Some(ImpactData {
                glyph: '*',
                color: Color::srgb(1.0, 0.5, 0.1),
                font_size: 16.0,
                duration: 0.3,
            }),
        }
    }

    /// Static animated flame impact at a grid position (for AoE fire effects).
    pub fn fire_impact(grid_pos: (i32, i32)) -> Self {
        let pos = grid_to_world_center(grid_pos.0, grid_pos.1);
        Self::SpriteImpact {
            world_pos: pos,
            sprite_paths: (0..3)
                .map(|i| format!("sprites/effects/flame_{}.png", i))
                .collect(),
            frame_rate: 10.0,
            duration: 0.4,
        }
    }

    /// Static animated lightning impact at a grid position (for AoE lightning effects).
    pub fn lightning_impact(grid_pos: (i32, i32)) -> Self {
        let pos = grid_to_world_center(grid_pos.0, grid_pos.1);
        Self::SpriteImpact {
            world_pos: pos,
            sprite_paths: (0..4)
                .map(|i| format!("sprites/effects/zap_{}.png", i))
                .collect(),
            frame_rate: 12.0,
            duration: 0.3,
        }
    }

    /// Single '*' impact flash at the given position.
    #[allow(dead_code)]
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
    Float {
        velocity: Vec2,
    },
    Projectile {
        destination: Vec2,
        speed: f32,
        on_impact: Option<ImpactData>,
    },
    SpriteProjectile {
        destination: Vec2,
        speed: f32,
        on_impact: Option<ImpactData>,
    },
    AnimatedSpriteProjectile {
        destination: Vec2,
        speed: f32,
        sprite_handles: Vec<Handle<Image>>,
        frame_rate: f32,
        frame_timer: f32,
        current_frame: usize,
        on_impact: Option<ImpactData>,
    },
    AnimatedImpact {
        sprite_handles: Vec<Handle<Image>>,
        frame_rate: f32,
        frame_timer: f32,
        current_frame: usize,
    },
    Impact,
}

// --- Setup ---

fn setup_particle_font(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(ParticleFont(asset_server.load("fonts/Macondo-Regular.ttf")));
}

// --- Helpers ---

/// Converts grid coordinates to world position, offset upward by half a tile
/// (used for floating text so it appears above the entity sprite).
pub fn grid_to_world(gx: i32, gy: i32) -> Vec2 {
    use crate::map::map::GRID_SIZE;
    Vec2::new(
        gx as f32 * GRID_SIZE.x,
        gy as f32 * GRID_SIZE.y + GRID_SIZE.y * 0.5,
    )
}

/// Converts grid coordinates to world position centered on the tile
/// (used for projectiles that should fly through tile centers).
pub fn grid_to_world_center(gx: i32, gy: i32) -> Vec2 {
    use crate::map::map::GRID_SIZE;
    Vec2::new(gx as f32 * GRID_SIZE.x, gy as f32 * GRID_SIZE.y)
}

/// Maps the direction from `src` to `dst` to a sprite index 0–7.
/// Convention: 0=↑, 1=↗, 2=→, 3=↘, 4=↓, 5=↙, 6=←, 7=↖ (clockwise from up).
fn direction_index_8(src: Vec2, dst: Vec2) -> usize {
    let d = dst - src;
    // atan2 gives angle from +X axis counter-clockwise; convert to clockwise-from-up index.
    let angle = d.y.atan2(d.x); // radians, -π..π
    // Map atan2 octant (CCW from +X) to our sprite index (CW from up).
    const OCTANT_TO_INDEX: [usize; 8] = [2, 1, 0, 7, 6, 5, 4, 3];
    // Snap to nearest 45° octant: shift by half-octant so boundaries fall between directions.
    let octant = ((angle / std::f32::consts::FRAC_PI_4).round() as i32).rem_euclid(8) as usize;
    OCTANT_TO_INDEX[octant]
}

/// Returns a thematic color for a given damage type (used for non-sprite spell projectile fallback).
pub fn damage_type_color(dt: crate::game::combat::DamageType) -> Color {
    use crate::game::combat::DamageType;
    match dt {
        DamageType::Fire => Color::srgb(1.0, 0.5, 0.1),
        DamageType::Lightning => Color::srgb(0.5, 0.8, 1.0),
        DamageType::Poison => Color::srgb(0.3, 0.9, 0.3),
        DamageType::Physical => Color::srgb(0.9, 0.9, 0.9),
        // `DamageType` is `#[non_exhaustive]` in the engine crate;
        // unknown future variants get a neutral fallback.
        _ => Color::srgb(0.9, 0.9, 0.9),
    }
}

fn spawn_impact_entity(
    commands: &mut Commands,
    font: &ParticleFont,
    world_pos: Vec2,
    impact: &ImpactData,
) {
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
    asset_server: Res<AssetServer>,
    map: Option<Res<Map>>,
    player_viewshed: Query<&Viewshed, With<Player>>,
) {
    for req in requests.read() {
        match req {
            ParticleRequest::FloatingText {
                world_pos,
                text,
                color,
                font_size,
            } => {
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
                    ParticleKind::Float {
                        velocity: FLOAT_VEL,
                    },
                ));
            }

            ParticleRequest::Projectile {
                source,
                destination,
                glyph,
                color,
                speed,
                on_impact,
            } => {
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

            ParticleRequest::SpriteProjectile {
                source,
                destination,
                sprite_path,
                speed,
                on_impact,
            } => {
                let dist = source.distance(*destination);
                let travel_duration = if *speed > 0.0 { dist / speed } else { 0.1 };
                commands.spawn((
                    Sprite {
                        image: asset_server.load(sprite_path),
                        custom_size: Some(Vec2::splat(16.0)),
                        ..default()
                    },
                    Transform::from_translation(source.extend(Z_PARTICLE)),
                    RenderLayers::layer(1),
                    GameEntityMarker,
                    ParticleEffect {
                        lifetime_remaining: travel_duration,
                        lifetime_total: travel_duration,
                    },
                    ParticleKind::SpriteProjectile {
                        destination: *destination,
                        speed: *speed,
                        on_impact: on_impact.clone(),
                    },
                ));
            }

            ParticleRequest::AnimatedSpriteProjectile {
                source,
                destination,
                sprite_paths,
                frame_rate,
                speed,
                on_impact,
            } => {
                let dist = source.distance(*destination);
                let travel_duration = if *speed > 0.0 { dist / speed } else { 0.1 };
                let handles: Vec<Handle<Image>> =
                    sprite_paths.iter().map(|p| asset_server.load(p)).collect();
                let first_handle = handles[0].clone();
                commands.spawn((
                    Sprite {
                        image: first_handle,
                        custom_size: Some(Vec2::splat(16.0)),
                        ..default()
                    },
                    Transform::from_translation(source.extend(Z_PARTICLE)),
                    RenderLayers::layer(1),
                    GameEntityMarker,
                    ParticleEffect {
                        lifetime_remaining: travel_duration,
                        lifetime_total: travel_duration,
                    },
                    ParticleKind::AnimatedSpriteProjectile {
                        destination: *destination,
                        speed: *speed,
                        sprite_handles: handles,
                        frame_rate: *frame_rate,
                        frame_timer: 0.0,
                        current_frame: 0,
                        on_impact: on_impact.clone(),
                    },
                ));
            }

            ParticleRequest::SpriteImpact {
                world_pos,
                sprite_paths,
                frame_rate,
                duration,
            } => {
                let handles: Vec<Handle<Image>> =
                    sprite_paths.iter().map(|p| asset_server.load(p)).collect();
                let first_handle = handles[0].clone();
                commands.spawn((
                    Sprite {
                        image: first_handle,
                        custom_size: Some(Vec2::splat(16.0)),
                        ..default()
                    },
                    Transform::from_translation(world_pos.extend(Z_PARTICLE)),
                    RenderLayers::layer(1),
                    GameEntityMarker,
                    ParticleEffect {
                        lifetime_remaining: *duration,
                        lifetime_total: *duration,
                    },
                    ParticleKind::AnimatedImpact {
                        sprite_handles: handles,
                        frame_rate: *frame_rate,
                        frame_timer: 0.0,
                        current_frame: 0,
                    },
                ));
            }

            ParticleRequest::Impact {
                world_pos,
                glyph,
                color,
                font_size,
                duration,
            } => {
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

            ParticleRequest::AoeImpact {
                center_grid,
                radius,
                glyph,
                color,
                font_size,
                duration,
            } => {
                let Some(map) = &map else {
                    continue;
                };
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
                        if let Some(vs) = viewshed
                            && !vs.visible_tiles.contains(&Point::new(tx, ty)) {
                                continue;
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
    mut particles: Query<(
        Entity,
        &mut Transform,
        &mut TextColor,
        &mut ParticleEffect,
        &mut ParticleKind,
    )>,
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
            ParticleKind::Projectile {
                destination,
                speed,
                on_impact,
            } => {
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
            ParticleKind::SpriteProjectile { .. }
            | ParticleKind::AnimatedSpriteProjectile { .. }
            | ParticleKind::AnimatedImpact { .. } => {
                // Handled by sprite_particle_update_system (won't match this query).
            }
            ParticleKind::Impact => {
                // Alpha fade handled above; no positional update needed
            }
        }
    }
}

/// Updates sprite-based projectile particles (arrows, etc.).
/// Separate from `particle_update_system` because these have `Sprite` instead of `TextColor`.
pub fn sprite_particle_update_system(
    mut commands: Commands,
    time: Res<Time>,
    font: Res<ParticleFont>,
    mut particles: Query<
        (
            Entity,
            &mut Transform,
            &mut Sprite,
            &mut ParticleEffect,
            &mut ParticleKind,
        ),
        Without<TextColor>,
    >,
) {
    let dt = time.delta_secs();
    for (entity, mut transform, mut sprite, mut effect, mut kind) in particles.iter_mut() {
        effect.lifetime_remaining -= dt;
        if effect.lifetime_remaining <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }

        let alpha = (effect.lifetime_remaining / effect.lifetime_total).clamp(0.0, 1.0);
        sprite.color = sprite.color.with_alpha(alpha);

        match &mut *kind {
            ParticleKind::SpriteProjectile {
                destination,
                speed,
                on_impact,
            } => {
                let current_pos = transform.translation.xy();
                let remaining = *destination - current_pos;
                let dir = remaining.normalize_or_zero();
                let step = dir * *speed * dt;

                if step.length() >= remaining.length() {
                    if let Some(impact) = on_impact.take() {
                        spawn_impact_entity(&mut commands, &font, *destination, &impact);
                    }
                    commands.entity(entity).despawn();
                } else {
                    transform.translation += step.extend(0.0);
                }
            }
            ParticleKind::AnimatedSpriteProjectile {
                destination,
                speed,
                sprite_handles,
                frame_rate,
                frame_timer,
                current_frame,
                on_impact,
            } => {
                // Frame cycling
                *frame_timer += dt;
                let frame_duration = 1.0 / *frame_rate;
                if *frame_timer >= frame_duration && !sprite_handles.is_empty() {
                    *frame_timer -= frame_duration;
                    *current_frame = (*current_frame + 1) % sprite_handles.len();
                    sprite.image = sprite_handles[*current_frame].clone();
                }

                // Movement
                let current_pos = transform.translation.xy();
                let remaining = *destination - current_pos;
                let dir = remaining.normalize_or_zero();
                let step = dir * *speed * dt;

                if step.length() >= remaining.length() {
                    if let Some(impact) = on_impact.take() {
                        spawn_impact_entity(&mut commands, &font, *destination, &impact);
                    }
                    commands.entity(entity).despawn();
                } else {
                    transform.translation += step.extend(0.0);
                }
            }
            ParticleKind::AnimatedImpact {
                sprite_handles,
                frame_rate,
                frame_timer,
                current_frame,
            } => {
                // Frame cycling only (no movement)
                *frame_timer += dt;
                let frame_duration = 1.0 / *frame_rate;
                if *frame_timer >= frame_duration && !sprite_handles.is_empty() {
                    *frame_timer -= frame_duration;
                    *current_frame = (*current_frame + 1) % sprite_handles.len();
                    sprite.image = sprite_handles[*current_frame].clone();
                }
            }
            _ => {}
        }
    }
}

/// Translates combat messages into ParticleRequests for visual feedback.
pub fn combat_particle_bridge_system(
    mut damage_messages: MessageReader<DamageEvent>,
    mut heal_messages: MessageReader<HealEvent>,
    mut miss_messages: MessageReader<MissMessage>,
    mut particle_writer: MessageWriter<ParticleRequest>,
    pos_query: Query<&Position>,
) {
    for msg in damage_messages.read() {
        if let Ok(pos) = pos_query.get(msg.target) {
            particle_writer.write(ParticleRequest::damage(
                grid_to_world(pos.x, pos.y),
                msg.amount,
            ));
        }
    }
    for msg in heal_messages.read() {
        if let Ok(pos) = pos_query.get(msg.target) {
            particle_writer.write(ParticleRequest::heal(
                grid_to_world(pos.x, pos.y),
                msg.amount,
            ));
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
                    combat_particle_bridge_system.after(CombatEventSet),
                    particle_spawn_system.after(combat_particle_bridge_system),
                    particle_update_system,
                    sprite_particle_update_system,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}
