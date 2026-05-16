use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::time::Timer;

use bracket_lib::prelude::Point;

use crate::{
    assets::{
        ItemManifest, ItemManifestHandle, ItemSpriteAssets, PlayerAsset, PlayerAssetHandle,
    },
    character::{
        compose_attributes, derive_stats, CharacterChoice, ClassManifest, ClassManifestHandle,
        RaceManifest, RaceManifestHandle, Race, RaceTrait,
    },
    components::{
        Collider, Faction, FactionKind, FloorEntityMarker, GameEntityMarker, InInventory, Inventory, Name, Position,
        VeiledTyrantFactions, Viewshed,
    },
    constants::Z_PLAYER,
    game::{
        TurnManager,
        actions::SpeedStats,
        combat::{Damage, DamageType, Health, HealthRegen, Resistances},
        items::Equipment,
        magic::StatusEffects,
        spawn_item,
        stats::{Armor, DamageBonus, Dodge, HitBonus},
    },
    map::dungeon::{PlayerSpawnPoint, SpawnDungeonMessage, SpawnDungeonSet, StairCooldown},
    map::map::GRID_SIZE,
};

use crate::assets::StartingItemDef;
use crate::game::items::ItemStack;

pub struct PlayerPlugin;

#[derive(Resource)]
pub struct MovementTimer(pub Timer);

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(MovementTimer(Timer::from_seconds(
            0.025,
            TimerMode::Repeating,
        )))
        .add_systems(
            Update,
            player_spawn_or_move_system
                .run_if(on_message::<SpawnDungeonMessage>)
                .after(SpawnDungeonSet),
        );
    }
}

#[derive(Component)]
pub struct Player;

pub fn player_spawn_or_move_system(
    mut commands: Commands,
    player_asset_handle: Res<PlayerAssetHandle>,
    player_assets: Res<Assets<PlayerAsset>>,
    item_manifest_handle: Res<ItemManifestHandle>,
    item_manifests: Res<Assets<ItemManifest>>,
    item_sprite_assets: Res<ItemSpriteAssets>,
    spawn_point: Res<PlayerSpawnPoint>,
    mut q_player: Query<(Entity, &mut Transform, &mut Position), With<Player>>,
    mut turn_manager: ResMut<TurnManager>,
    ascii_font: Option<Res<crate::game::ascii_mode::AsciiFont>>,
    character_choice: Res<CharacterChoice>,
    race_manifest_handle: Res<RaceManifestHandle>,
    race_manifests: Res<Assets<RaceManifest>>,
    class_manifest_handle: Res<ClassManifestHandle>,
    class_manifests: Res<Assets<ClassManifest>>,
) {
    let player_asset = player_assets
        .get(&player_asset_handle.0)
        .expect("Player asset not loaded");

    let new_grid_pos = Position {
        x: spawn_point.0.x,
        y: spawn_point.0.y,
    };

    if let Ok((player_entity, mut player_tf, mut player_pos)) = q_player.single_mut() {
        info!(
            "player_spawn_or_move: teleporting from ({}, {}) to ({}, {})",
            player_pos.x, player_pos.y, spawn_point.0.x, spawn_point.0.y
        );
        // Update Transform immediately so move_camera snaps this frame without
        // waiting for sync_entity_transforms, which may run before this system.
        player_tf.translation.x = spawn_point.0.x as f32 * GRID_SIZE.x;
        player_tf.translation.y = spawn_point.0.y as f32 * GRID_SIZE.y;
        *player_pos = new_grid_pos;
        // Prevent player_stair_system from immediately re-triggering if
        // the player spawns on a stair tile (floor transitions).
        commands.entity(player_entity).insert(StairCooldown);
    } else {
        let tile_size = UVec2::new(32, 32);
        let scale_x = GRID_SIZE.x / tile_size.x as f32;
        let scale_y = GRID_SIZE.y / tile_size.y as f32;

        // Resolve race + class assets from the player's character-creation
        // choice. The manifests are guaranteed loaded by the time we get here
        // because check_assets_loaded gates the Menu→InGame transition on them.
        let race_manifest = race_manifests
            .get(&race_manifest_handle.0)
            .expect("Race manifest not loaded");
        let class_manifest = class_manifests
            .get(&class_manifest_handle.0)
            .expect("Class manifest not loaded");
        let race_id = character_choice.race.name().to_lowercase();
        let class_id = character_choice.class.name().to_lowercase();
        let race_asset = race_manifest
            .races
            .get(&race_id)
            .unwrap_or_else(|| panic!("races.ron missing entry for {race_id}"));
        let class_asset = class_manifest
            .classes
            .get(&class_id)
            .unwrap_or_else(|| panic!("classes.ron missing entry for {class_id}"));

        // Spawn starting items from the chosen class's kit. The legacy
        // `player.ron` `starting_items` field is now unused by the live
        // character-creation flow — kept around only so player.ron stays
        // parseable; the runtime kit is class-driven.
        let starting_items = spawn_starting_items(
            &mut commands,
            &class_asset.starting_kit,
            &item_manifests,
            &item_manifest_handle,
            &item_sprite_assets,
        );

        let attributes = compose_attributes(race_asset, class_asset);
        // Spawn always starts the player at level 1 — XP/level progression
        // is owned by the XP system, not the spawner. The save-load path
        // restores the saved level (and recomputes HP from the formula
        // at that level) in `apply_player_load_system`.
        //
        // Fighting passed as starting-skill level: class.starting_skills
        // contributes a chargen Fighting bonus the spawn HP needs to
        // reflect. The XP/level system recomputes HP on every level-up
        // using the current Skills component.
        let starting_fighting = class_asset.starting_skills.fighting as f32;
        let derived = derive_stats(race_asset, &attributes, 1, starting_fighting);

        // Apply Elf's Keen Senses (+2 vision) at spawn. Other race effects
        // (Stoneblood poison resist) are applied below.
        let mut viewshed_range = if player_asset.viewshed_range > 0 {
            player_asset.viewshed_range
        } else {
            8
        };
        if character_choice.race.racial_trait() == RaceTrait::KeenSenses {
            viewshed_range += 2;
        }

        // Resistances inherit the player-asset defaults, then Stoneblood
        // stacks 50% poison resistance on top.
        let mut resistances = Resistances::default();
        if character_choice.race.racial_trait() == RaceTrait::Stoneblood {
            *resistances.0.entry(DamageType::Poison).or_insert(0) += 50;
        }

        let player_entity = commands
            .spawn((
                Player,
                Name(player_asset.name.clone()),
                GameEntityMarker,
                Collider,
                new_grid_pos,
                Viewshed::new(viewshed_range),
                roguelike_engine::components::FovRevealsMap,
                Inventory {
                    items: starting_items,
                    capacity: 20,
                },
                Equipment::default(),
            ))
            .insert((
                Health {
                    current: derived.max_hp,
                    max: derived.max_hp,
                },
                HealthRegen {
                    regen_rate: player_asset.regen_rate,
                    regen_accumulator: 0,
                },
                Damage(player_asset.damage.clone()),
                Armor(player_asset.armor),
                crate::game::stats::Block(0),
                crate::game::stats::MaxShieldBlocks(0),
                crate::game::stats::ShieldBlocksUsed(0),
                Dodge(player_asset.dodge + derived.dodge),
                // HitBonus / DamageBonus start at 0. The hit-check and
                // damage-roll systems add STR_mod (melee) or DEX_mod
                // (ranged) dynamically based on AttackIntentMessage.source.
                // Class attack/dodge fudge factors no longer exist —
                // every per-class combat number derives from stats.
                HitBonus(0),
                DamageBonus(0),
                SpeedStats::default(),
            ))
            .insert((
                StatusEffects::default(),
                Faction(VeiledTyrantFactions::player()),
                resistances,
                character_choice.race,
                character_choice.class,
                attributes,
                // Phase 2: level + XP. Spawned at level 1 with 0 XP. The
                // save-load path overrides these from PlayerSaveData.
                crate::game::xp::Level(1),
                crate::game::xp::Experience(0),
                // Phase 3: skill components. Skills levels are seeded
                // from class.starting_skills via SkillXp (so the XP pool
                // and level-from-xp invariant holds from frame zero).
                // SkillTraining defaults all skills to Normal mode.
                build_starting_skill_xp(&class_asset.starting_skills),
                build_starting_skills(&class_asset.starting_skills),
                crate::game::skills::SkillTraining::new(),
            ))
            .insert((
                Transform {
                    translation: Vec3::new(0.0, 0.0, Z_PLAYER),
                    scale: Vec3::new(scale_x, scale_y, 1.0),
                    ..Default::default()
                },
                RenderLayers::layer(1),
            ))
            .id();

        commands.entity(player_entity).insert(crate::game::ascii_mode::AsciiDisplay {
            ch: if player_asset.ascii_char.is_empty() { "@".to_string() } else { player_asset.ascii_char.clone() },
            color: player_asset.ascii_fg,
        });

        if let Some(ref font) = ascii_font {
            crate::game::spawner::attach_ascii_glyph(
                &mut commands,
                player_entity,
                &player_asset.ascii_char,
                player_asset.ascii_fg,
                &font.0,
                Vec3::new(scale_x, scale_y, 1.0),
            );
        }

        turn_manager.add_entity(player_entity);
    }
}

/// Build a `Skills` component from the class's starting distribution.
/// Floors negative values to 0 (the schema allows negatives for future
/// classes but Skills::set already clamps; this just makes the contract
/// explicit at the spawn boundary).
fn build_starting_skills(
    dist: &crate::character::SkillDistribution,
) -> crate::game::skills::Skills {
    let mut skills = crate::game::skills::Skills::new();
    for (skill, level) in dist.iter() {
        skills.set(skill, (level as f32).max(0.0));
    }
    skills
}

/// Build a `SkillXp` so that `xp_to_level(xp) == starting_skills_level`
/// for each skill. Looks up the XP threshold for the target integer
/// level in the DCSS table. Keeps Skills and SkillXp in lockstep at
/// spawn so the `update_skill_levels` system has nothing to do on
/// frame zero.
fn build_starting_skill_xp(
    dist: &crate::character::SkillDistribution,
) -> crate::game::skills::SkillXp {
    use crate::game::skills::{Skill, SkillXp};

    let mut sx = SkillXp::new();
    // Pre-Phase-3 there's no public accessor for the threshold table.
    // We compute the threshold by binary search: find the lowest
    // cumulative XP such that xp_to_level(xp) >= target. For integer
    // targets the answer is the threshold exactly.
    for (skill, level) in dist.iter() {
        let target = level.max(0) as u32;
        if target == 0 {
            continue;
        }
        // The DCSS XP threshold for reaching integer level N is the
        // first u64 in the table at index N-1. We can't access the
        // constant directly from here, so derive it by binary search
        // (cheap; runs once at spawn).
        let mut lo: u64 = 0;
        let mut hi: u64 = 30_000;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if crate::game::skills::xp_to_level(mid) < target as f32 {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        sx.add(skill, lo);
        let _ = Skill::ALL; // silence unused-import warning until later
    }
    sx
}

/// Spawn starting inventory items from the player asset manifest.
fn spawn_starting_items(
    commands: &mut Commands,
    item_defs: &[StartingItemDef],
    item_manifests: &Res<Assets<ItemManifest>>,
    item_manifest_handle: &Res<ItemManifestHandle>,
    item_sprite_assets: &Res<ItemSpriteAssets>,
) -> Vec<Entity> {
    let mut items = Vec::new();
    for def in item_defs {
        if let Some(entity) = spawn_item(
            commands,
            &def.name,
            &Point::new(0, 0),
            item_manifests,
            item_manifest_handle,
            item_sprite_assets,
            None,
            None,
        ) {
            commands
                .entity(entity)
                .insert(InInventory)
                .insert(Visibility::Hidden)
                .remove::<FloorEntityMarker>()
                .remove::<Position>();
            if def.count > 1 {
                let max_stack = item_manifests
                    .get(&item_manifest_handle.0)
                    .and_then(|m| m.items.get(def.name.as_str()))
                    .map(|a| a.max_stack)
                    .unwrap_or(1);
                commands.entity(entity).insert(ItemStack {
                    count: def.count,
                    max_stack,
                });
            }
            items.push(entity);
        }
    }
    items
}
