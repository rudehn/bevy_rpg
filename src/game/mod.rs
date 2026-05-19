use crate::game::abilities::AbilitiesPlugin;
use crate::game::effects::EffectsPlugin;
use crate::game::enchantment::EnchantmentPlugin;
use crate::game::factions::FactionsPlugin;
use crate::game::ranged::RangedPlugin;
use crate::game::squad::SquadPlugin;
use crate::game::targeting::TargetingPlugin;
use crate::{
    assets::{ItemManifest, ItemManifestHandle, ItemSpriteAssets},
    components::{GameEntityMarker, Name, Position, Viewshed},
    ui::game_log::GameLogMessage,
    game::{
        camera::{move_camera, toggle_main_camera_visibility},
        combat::{CombatDamageSet, GameCombatPlugin, DeathEvent, GameRng, death_system, Health},
        items::{Equipment, ItemsPlugin, LootTable},
        magic::MagicPlugin,
        particles::ParticlesPlugin,
        stats::StatsPlugin,
        systems::{fov_update_system, mark_moved_viewsheds_dirty, sync_entity_transforms, update_monster_visibility, update_item_visibility, update_prop_visibility},
        turns::TurnOrderPlugin,
    },
    map::{
        dungeon::{DungeonPlugin, Floor},
        light::LightPlugin,
        map::MapPlugin,
    },
    player::{PlayerPlugin, player_spawn_or_move_system},
    ui::game_log::GameLog,
};
use bevy::prelude::*;

/// Captures end-of-run statistics for the death/victory screen.
#[allow(dead_code)]
#[derive(Resource, Default, Clone)]
pub struct RunSummary {
    pub floor_reached: u32,
    pub cause: String,
    pub victory: bool,
    pub enemies_killed: u32,
}

/// Tracks cumulative run statistics (reset each new game).
#[derive(Resource, Default, Clone)]
pub struct RunStats {
    pub enemies_killed: u32,
    /// Name of the last entity that dealt damage to the player.
    pub last_hit_by: String,
}
pub mod abilities;
pub mod actions;
pub mod ai;
pub mod ascii_mode;
pub mod camera;
pub mod combat;
pub mod effects;
pub mod enchantment;
pub mod factions;
pub mod fire;
pub mod fleeing;
pub mod gas;
pub mod items;
pub mod magic;
pub mod particles;
pub mod prop_effects;
pub mod ranged;
pub mod spawner;
pub mod squad;
pub mod staves;
pub mod stats;
pub mod stealth;
pub mod systems;
pub mod skills;
pub mod tactics;
pub mod targeting;
pub mod tile_promotion;
pub mod turns;
pub mod xp;
pub mod water;
pub use ai::*;
pub use spawner::*;
pub use turns::*;

use crate::map::map::DungeonECSMap;
use crate::map::tile::TileMarker;

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    Loading,
    Menu,
    /// Race / Class / Attribute allocation screen between Menu and InGame.
    CharacterCreation,
    InGame,
    GameOver,
    Victory,
}

#[derive(SubStates, Debug, Clone, PartialEq, Eq, Hash, Default)]
#[source(AppState = AppState::InGame)]
pub enum InGameState {
    #[default]
    Running,
    Inventory,
    Targeting,
    LogHistory,
    EnchantSelect,
    Help,
    ChasmConfirm,
    /// DCSS-style ASI prompt — drains a `PendingAsi` component on the
    /// player. Blocks input until all queued points are spent.
    AsiSelect,
    /// Character info screen (Phase 2). Bound to C.
    CharacterInfo,
    /// Skill training screen (Phase 3). Bound to M.
    SkillScreen,
    /// Debug cheat menu. Bound to Backslash.
    CheatMenu,
}

pub struct GamePlugin;
impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RunSummary>()
            .init_resource::<systems::Omniscient>()
            .init_resource::<RunStats>()
            .add_sub_state::<InGameState>()
            .add_plugins((
                LightPlugin,
                MapPlugin,
                PlayerPlugin,
                DungeonPlugin,
                TurnOrderPlugin,
                // Engine's CombatPlugin: registers DamageEvent/DeathEvent/HealEvent
                // message types + damage_application_system + heal_application_system
                roguelike_engine::combat::events::CombatPlugin,
                // Engine's FovPlugin: registers fov_update_system in FovSet
                roguelike_engine::components::FovPlugin,
                GameCombatPlugin,
                StatsPlugin,
                ItemsPlugin,
                MagicPlugin,
                ParticlesPlugin,
                SquadPlugin,
                TargetingPlugin,
                AbilitiesPlugin,
            ))
            .add_plugins((
                RangedPlugin,
                EffectsPlugin,
                FactionsPlugin,
                EnchantmentPlugin,
                crate::game::staves::StavesPlugin,
                crate::game::prop_effects::PropEffectsPlugin,
                water::WaterPlugin,
                ascii_mode::AsciiModePlugin,
                crate::game::xp::XpPlugin,
                crate::game::skills::SkillsPlugin,
                crate::game::stealth::StealthPlugin,
                crate::game::tactics::TacticsPlugin,
                crate::game::fleeing::FleeingPlugin,
            ))
            // Position→Transform sync and camera run whenever in-game, including Targeting state.
            //
            // Ordered .after(ProcessingPhase::ResolveActions) so all
            // turn-resolution handlers that may mutate `Position`
            // (handle_movement, knockback, blink, chasm-fall, monster
            // AI) have completed before we read positions. Without
            // this, Bevy was free to schedule sync_entity_transforms /
            // mark_moved_viewsheds_dirty BEFORE handle_movement in the
            // same Update tick — `Changed<Position>` would miss the
            // change, the FOV dirty flag stayed false, and
            // render_tile_ascii skipped the new player tile for one
            // frame → player sprite flickers when moving.
            //
            // move_camera runs after player_spawn_or_move_system so the camera snaps to the new
            // floor position in the same frame the player is teleported (floor transitions).
            .add_systems(
                Update,
                (
                    sync_entity_transforms
                        .after(CombatDamageSet)
                        .after(crate::game::turns::ProcessingPhase::ResolveActions),
                    move_camera
                        .after(sync_entity_transforms)
                        .after(player_spawn_or_move_system),
                )
                    .run_if(in_state(AppState::InGame)),
            )
            // Configure engine's FovSet to run after transforms sync and
            // before visibility systems + squad alerting.
            .configure_sets(
                Update,
                roguelike_engine::components::FovSet
                    .after(sync_entity_transforms)
                    .run_if(in_state(AppState::InGame)),
            )
            // Mark any moved entity's viewshed dirty so the engine's FOV
            // system recomputes visibility on the same frame the move
            // resolves. Without this, FOV only updates when an unrelated
            // system happens to flip viewshed.dirty (door opens, tile
            // mutations, vision-bonus equip), producing the "FOV updates
            // randomly after several turns" symptom.
            //
            // Must run after ProcessingPhase::ResolveActions for the
            // same reason as sync_entity_transforms above — without it,
            // Bevy can schedule this before handle_movement and the
            // Changed<Position> filter sees nothing.
            .add_systems(
                Update,
                mark_moved_viewsheds_dirty
                    .after(crate::game::turns::ProcessingPhase::ResolveActions)
                    .before(roguelike_engine::components::FovSet)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    update_monster_visibility
                        .run_if(|query: Query<(), Changed<Position>>| !query.is_empty())
                        .after(roguelike_engine::components::FovSet),
                    update_item_visibility
                        .run_if(|vs_query: Query<(), Changed<Viewshed>>, item_query: Query<(), (Changed<Visibility>, With<crate::components::Item>)>| {
                            !vs_query.is_empty() || !item_query.is_empty()
                        })
                        .after(roguelike_engine::components::FovSet),
                    update_prop_visibility
                        .run_if(|query: Query<(), Changed<Viewshed>>| !query.is_empty())
                        .after(roguelike_engine::components::FovSet),
                    loot_drop_system.after(CombatDamageSet),
                    crate::game::combat::drop_inventory_on_death.after(CombatDamageSet),
                    drop_equipment_on_death.after(CombatDamageSet),
                    death_system
                        .after(loot_drop_system)
                        .after(crate::game::combat::drop_inventory_on_death)
                        .after(drop_equipment_on_death),
                )
                    .run_if(in_state(InGameState::Running)),
            )
            .add_systems(
                Update,
                toggle_main_camera_visibility.run_if(state_changed::<AppState>),
            )
            .add_systems(OnEnter(AppState::GameOver), (despawn_game_entities, despawn_map))
            .add_systems(OnEnter(AppState::Victory), (despawn_game_entities, despawn_map))
            .init_resource::<TurnManager>()
            .init_resource::<crate::game::actions::PendingChasmFall>()
            .init_resource::<tile_promotion::PromotionCooldown>()
            .init_resource::<fire::FireTiles>()
            .init_resource::<gas::GasTiles>()
            .init_resource::<water::WaterTiles>()
            ;
    }
}

fn loot_drop_system(
    mut death_events: MessageReader<DeathEvent>,
    loot_query: Query<(&Position, &LootTable, &Name)>,
    mut commands: Commands,
    mut game_rng: ResMut<GameRng>,
    mut log_writer: MessageWriter<GameLogMessage>,
    spawner: crate::game::spawner::ItemSpawner,
) {
    use bracket_lib::prelude::Point;
    for event in death_events.read() {
        let Ok((position, loot_table, name)) = loot_query.get(event.entity) else {
            continue;
        };
        let spawn_point = Point::new(position.x, position.y);
        for entry in &loot_table.entries {
            let roll = game_rng.0.range(0, 100);
            if (roll as f32) < entry.spawn_chance * 100.0 {
                let count = if entry.count_max > 1 {
                    game_rng.0.range(entry.count_min, entry.count_max + 1)
                } else {
                    1
                };
                if let Some(entity) = spawner.try_spawn(&mut commands, &entry.item, &spawn_point, None)
                    && count > 1
                {
                    let max_stack = spawner.max_stack_for(&entry.item);
                    commands
                        .entity(entity)
                        .insert(crate::game::items::ItemStack { count, max_stack });
                }
                let count_str = if count > 1 { format!(" (x{})", count) } else { String::new() };
                log_writer.write(GameLogMessage(format!(
                    "{} dropped a {}{}.", name.0, entry.item, count_str
                )));
            }
        }
    }
}

/// On death, drop everything the entity had equipped onto the floor at
/// its death position. Mirrors the existing `drop_inventory_on_death`
/// for `Inventory.items`, but handles `Equipment` slots — including the
/// minimal item entities the monster loadout system spawns (no Position,
/// no rendering). For those, the simplest path is to despawn the bare
/// entity and re-`spawn_item` a full floor-ready one at the death tile.
///
/// Loadout items are sterile (no enchantment, no runic — monsters don't
/// enchant their gear), so re-spawning loses nothing. If monster
/// equipment ever gains state (enchantment, durability), this is the
/// system to revisit.
fn drop_equipment_on_death(
    mut commands: Commands,
    dead_query: Query<(&Health, &Position, &Equipment, &Name)>,
    item_name_query: Query<&Name>,
    spawner: crate::game::spawner::ItemSpawner,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    use bracket_lib::prelude::Point;
    for (health, pos, equipment, owner_name) in dead_query.iter() {
        if health.current > 0 {
            continue;
        }
        let drop_point = Point::new(pos.x, pos.y);
        let slot_entities = [
            equipment.weapon,
            equipment.offhand,
            equipment.helm,
            equipment.chest,
            equipment.gloves,
            equipment.boots,
            equipment.ring_l,
            equipment.ring_r,
            equipment.amulet,
        ];
        for slot_entity in slot_entities.into_iter().flatten() {
            let Ok(item_name) = item_name_query.get(slot_entity) else { continue; };
            let item_name_str = item_name.0.clone();
            // Despawn the bare equipped entity; spawn a fresh floor item.
            commands.entity(slot_entity).despawn();
            if spawner.try_spawn(&mut commands, &item_name_str, &drop_point, None).is_some() {
                log_writer.write(GameLogMessage(format!(
                    "{} drops a {}.", owner_name.0, item_name_str
                )));
            }
        }
    }
}

fn despawn_game_entities(
    mut commands: Commands,
    game_entities_query: Query<Entity, With<GameEntityMarker>>,
    mut turn_manager: ResMut<TurnManager>,
    mut floor: ResMut<Floor>,
    mut game_log: ResMut<GameLog>,
) {
    info!("Despawning all game entities...");

    for entity in game_entities_query.iter() {
        commands.entity(entity).despawn();
    }

    // Reset turn manager to clear any remaining entities in the queue
    *turn_manager = TurnManager::default();
    *floor = Floor::default();
    game_log.entries.clear();

    info!("Finished despawning game entities.");
}

fn despawn_map(
    mut commands: Commands,
    q_map: Query<Entity, With<DungeonECSMap>>,
    q_tiles: Query<Entity, With<TileMarker>>,
) {
    for entity in q_map.iter() {
        commands.entity(entity).despawn();
    }
    for entity in q_tiles.iter() {
        commands.entity(entity).despawn();
    }
}
