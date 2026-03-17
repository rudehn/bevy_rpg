use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bracket_lib::prelude::Point;

use crate::{
    assets::{
        AbilityDef, MonsterAsset, MonsterManifest, MonsterManifestHandle, MonsterSpriteAssets,
        ItemManifest, ItemManifestHandle, ItemSpriteAssets,
        PropManifest, PropManifestHandle, PropSpriteAssets,
    },
    components::{
        Ammo, Collider, Faction, FactionKind, FinalBoss, FloorEntityMarker, GameEntityMarker, Monster, Name, Position, Prop, Viewshed, Item,
    },
    constants::{TILE_SIZE_X, TILE_SIZE_Y, Z_MONSTER, Z_ITEM},
    game::{
        MonsterAI, TurnManager,
        actions::SpeedStats,
        combat::{Damage, DamageType, DamageTypeTag, Health, HealthRegen, Resistances, ResistanceLevel},
        items::{ItemProperties, ItemStack, LootEntry, LootTable},
        magic::{ActiveSpells, KnownSpells, ManaRegen, SpellCooldowns, MAX_SPELL_SLOTS},
        ranged::RangedCapable,
        stats::{Armor, Dodge, Mana},
    },
    map::map::GRID_SIZE,
};

pub fn spawn_monster(
    commands: &mut Commands,
    spawn_point: &Point,
    turn_manager: &mut ResMut<TurnManager>,
    monster_asset: &MonsterAsset,
    monster_sprite_assets: &Res<MonsterSpriteAssets>,
) -> Option<Entity> {
    let tile_size = monster_asset.tile_size.unwrap_or(UVec2::new(32, 32));
    let scale_x = TILE_SIZE_X as f32 / tile_size.x as f32;
    let scale_y = TILE_SIZE_Y as f32 / tile_size.y as f32;

    let new_pos = Transform {
        translation: Vec3::new(
            spawn_point.x as f32 * GRID_SIZE.x,
            spawn_point.y as f32 * GRID_SIZE.y,
            Z_MONSTER,
        ),
        scale: Vec3::new(scale_x, scale_y, 1.0),
        ..Default::default()
    };
    let new_grid_pos = Position {
        x: spawn_point.x,
        y: spawn_point.y,
    };

    let (texture_path, index) = crate::assets::parse_sprite_path(&monster_asset.sprite);

    let Some(texture_handle) = monster_sprite_assets.handles.get(texture_path).cloned() else {
        error!("Missing monster sprite texture: '{}'", texture_path);
        return None;
    };
    let Some(layout_handle) = monster_sprite_assets.layouts.get(texture_path).cloned() else {
        error!("Missing monster sprite layout: '{}'", texture_path);
        return None;
    };

    let monster_entity = commands
        .spawn((
            Monster,
            GameEntityMarker,
            FloorEntityMarker,
            Name(monster_asset.name.clone()),
            MonsterAI::default(),
            Collider,
            new_grid_pos,
            new_pos,
            Viewshed::new(monster_asset.vision.max(2)),
            Faction(FactionKind::Monster),
        ))
        .insert((
            Health {
                current: monster_asset.base_hp,
                max: monster_asset.base_hp,
            },
            Damage(monster_asset.damage.clone()),
            SpeedStats::default(),
            Armor(monster_asset.base_armor),
            Dodge(0),
        ))
        .insert((
            Sprite::from_atlas_image(
                texture_handle,
                TextureAtlas {
                    index,
                    layout: layout_handle,
                },
            ),
            Visibility::Hidden,
            RenderLayers::layer(1),
        ))
        .id();

    if let Some(regen_rate) = monster_asset.regen {
        commands.entity(monster_entity).insert(HealthRegen {
            regen_rate,
            regen_accumulator: 0,
        });
    }

    if !monster_asset.loot_table.is_empty() {
        let entries = monster_asset
            .loot_table
            .iter()
            .map(|e| LootEntry {
                item: e.item.clone(),
                spawn_chance: e.spawn_chance,
                count_min: e.count_min,
                count_max: e.count_max,
            })
            .collect();
        commands.entity(monster_entity).insert(LootTable { entries });
    }

    // Caster monsters: add mana pool and regen components.
    if !monster_asset.spells.is_empty() || monster_asset.mana > 0 {
        let mana_max = monster_asset.mana;
        commands.entity(monster_entity).insert((
            Mana {
                current: mana_max,
                max: mana_max,
            },
            ManaRegen::default(),
        ));
    }

    if monster_asset.ranged_range > 0 {
        commands
            .entity(monster_entity)
            .insert(RangedCapable { range: monster_asset.ranged_range });
    }

    if !monster_asset.spells.is_empty() {
        let mut slots = vec![None; MAX_SPELL_SLOTS];
        for (i, spell_id) in monster_asset.spells.iter().enumerate() {
            if i < MAX_SPELL_SLOTS {
                slots[i] = Some(spell_id.clone());
            }
        }
        commands.entity(monster_entity).insert((
            KnownSpells { spells: monster_asset.spells.clone() },
            ActiveSpells { slots },
            SpellCooldowns::default(),
        ));
    }

    // Damage type tag (for melee attacks)
    let dmg_type = DamageType::from_str(&monster_asset.damage_type);
    if dmg_type != DamageType::Physical {
        commands.entity(monster_entity).insert(DamageTypeTag(dmg_type));
    }

    // Resistances
    if !monster_asset.resistances.is_empty() {
        let mut map = std::collections::HashMap::new();
        for (dt_str, rl_str) in &monster_asset.resistances {
            map.insert(DamageType::from_str(dt_str), ResistanceLevel::from_str(rl_str));
        }
        commands.entity(monster_entity).insert(Resistances(map));
    }

    // Boss marker + AI
    if monster_asset.is_boss {
        commands.entity(monster_entity).insert((
            FinalBoss,
            crate::game::boss::BossAI::default(),
        ));
    }

    // Abilities — convert AbilityDef entries into ECS components
    use crate::game::abilities::*;
    for ability in &monster_asset.abilities {
        match ability {
            AbilityDef::BurningStrike { damage_per_turn, duration, chance } => {
                commands.entity(monster_entity).insert(BurningStrike {
                    damage_per_turn: *damage_per_turn,
                    duration: *duration,
                    chance: *chance,
                });
            }
            AbilityDef::StunningBlow { duration, chance } => {
                commands.entity(monster_entity).insert(StunningBlow {
                    duration: *duration,
                    chance: *chance,
                });
            }
            AbilityDef::SlowStrike { duration, chance } => {
                commands.entity(monster_entity).insert(SlowStrike {
                    duration: *duration,
                    chance: *chance,
                });
            }
            AbilityDef::LifeDrain { percent } => {
                commands.entity(monster_entity).insert(LifeDrain {
                    percent: *percent,
                });
            }
            AbilityDef::Knockback { distance, chance } => {
                commands.entity(monster_entity).insert(Knockback {
                    distance: *distance,
                    chance: *chance,
                });
            }
            AbilityDef::RoughBody { damage } => {
                commands.entity(monster_entity).insert(RoughBody {
                    damage: *damage,
                });
            }
            AbilityDef::Enrage { threshold_percent } => {
                commands.entity(monster_entity).insert(Enrage {
                    threshold_percent: *threshold_percent,
                });
            }
            AbilityDef::ExplodeOnDeath { damage, radius } => {
                commands.entity(monster_entity).insert(ExplodeOnDeath {
                    damage: *damage,
                    radius: *radius,
                });
            }
            AbilityDef::SummonOnDeath { monster, count } => {
                commands.entity(monster_entity).insert(SummonOnDeath {
                    monster_name: monster.clone(),
                    count: *count,
                });
            }
            AbilityDef::PackTactics => {
                commands.entity(monster_entity).insert(PackTactics);
            }
            AbilityDef::WarCry { radius, duration } => {
                commands.entity(monster_entity).insert(WarCry {
                    radius: *radius,
                    duration: *duration,
                    activated: false,
                });
            }
            AbilityDef::Rally { radius, armor_bonus } => {
                commands.entity(monster_entity).insert(Rally {
                    radius: *radius,
                    armor_bonus: *armor_bonus,
                });
            }
            AbilityDef::Terrify { radius } => {
                commands.entity(monster_entity).insert(Terrify {
                    radius: *radius,
                });
            }
        }
    }

    turn_manager.add_entity(monster_entity);
    Some(monster_entity)
}

pub fn spawn_monster_by_name(
    commands: &mut Commands,
    monster_name: &str,
    spawn_point: &Point,
    turn_manager: &mut ResMut<TurnManager>,
    monster_manifests: &Res<Assets<MonsterManifest>>,
    monster_manifest_handle: &Res<MonsterManifestHandle>,
    monster_sprite_assets: &Res<MonsterSpriteAssets>,
) -> Option<Entity> {
    if let Some(manifest) = monster_manifests.get(&monster_manifest_handle.0) {
        if let Some(monster_asset) = manifest.monsters.get(monster_name) {
            spawn_monster(
                commands,
                spawn_point,
                turn_manager,
                monster_asset,
                monster_sprite_assets,
            )
        } else {
            warn!("Monster '{}' not found in manifest.", monster_name);
            None
        }
    } else {
        error!("Monster manifest not loaded.");
        None
    }
}

pub fn spawn_item(
    commands: &mut Commands,
    item_name: &str,
    spawn_point: &Point,
    item_manifests: &Res<Assets<ItemManifest>>,
    item_manifest_handle: &Res<ItemManifestHandle>,
    item_sprite_assets: &Res<ItemSpriteAssets>,
) -> Option<Entity> {
    let Some(manifest) = item_manifests.get(&item_manifest_handle.0) else {
        return None;
    };
    let Some(asset) = manifest.items.get(item_name) else {
        warn!("Item '{}' not found in manifest.", item_name);
        return None;
    };

    let (texture_path, index) = crate::assets::parse_sprite_path(&asset.sprite);

    let Some(texture_handle) = item_sprite_assets.handles.get(texture_path).cloned() else {
        error!("Missing item sprite texture: '{}'", texture_path);
        return None;
    };
    let Some(layout_handle) = item_sprite_assets.layouts.get(texture_path).cloned() else {
        error!("Missing item sprite layout: '{}'", texture_path);
        return None;
    };

    // Determine scale to fit one game map tile (GRID_SIZE)
    let tile_size = asset.tile_size.unwrap_or(UVec2::new(32, 32));
    let scale_x = GRID_SIZE.x / tile_size.x as f32;
    let scale_y = GRID_SIZE.y / tile_size.y as f32;

    let mut entity = commands.spawn((
        Item,
        Name(asset.name.clone()),
        GameEntityMarker,
        FloorEntityMarker,
        Position { x: spawn_point.x, y: spawn_point.y },
        Sprite::from_atlas_image(
            texture_handle,
            TextureAtlas {
                index,
                layout: layout_handle,
            },
        ),
        Transform {
            translation: Vec3::new(
                spawn_point.x as f32 * GRID_SIZE.x,
                spawn_point.y as f32 * GRID_SIZE.y,
                Z_ITEM,
            ),
            scale: Vec3::new(scale_x, scale_y, 1.0),
            ..Default::default()
        },
        Visibility::Hidden,
        RenderLayers::layer(1),
    ));

    entity.insert(ItemProperties {
        kind: asset.item_kind.clone(),
        armor_slot: asset.armor_slot.clone(),
        damage: asset.damage.clone(),
        defense: asset.defense,
        rarity: asset.rarity.clone(),
        effect: asset.effect.clone(),
        weapon_range: asset.weapon_range,
    });

    entity.insert(ItemStack { count: 1, max_stack: asset.max_stack });

    if asset.is_ammo {
        entity.insert(Ammo);
    }

    Some(entity.id())
}

pub fn spawn_prop(
    commands: &mut Commands,
    prop_name: &str,
    spawn_point: &Point,
    prop_manifests: &Res<Assets<PropManifest>>,
    prop_manifest_handle: &Res<PropManifestHandle>,
    prop_sprite_assets: &Res<PropSpriteAssets>,
) -> Option<Entity> {
    let manifest = prop_manifests.get(&prop_manifest_handle.0)?;
    let asset = manifest.props.get(prop_name).or_else(|| {
        warn!("Prop '{}' not found in manifest.", prop_name);
        None
    })?;

    let (texture_path, index) = crate::assets::parse_sprite_path(&asset.sprite);

    let texture_handle = prop_sprite_assets.handles.get(texture_path).cloned().or_else(|| {
        error!("Missing prop sprite texture: '{}'", texture_path);
        None
    })?;
    let layout_handle = prop_sprite_assets.layouts.get(texture_path).cloned().or_else(|| {
        error!("Missing prop sprite layout: '{}'", texture_path);
        None
    })?;

    let tile_size = asset.tile_size.unwrap_or(UVec2::new(16, 16));
    let scale_x = TILE_SIZE_X as f32 / tile_size.x as f32;
    let scale_y = TILE_SIZE_Y as f32 / tile_size.y as f32;

    let mut entity = commands.spawn((
        Prop,
        Name(asset.name.clone()),
        GameEntityMarker,
        FloorEntityMarker,
        Position { x: spawn_point.x, y: spawn_point.y },
        Sprite::from_atlas_image(
            texture_handle,
            TextureAtlas {
                index,
                layout: layout_handle,
            },
        ),
        Transform {
            translation: Vec3::new(
                spawn_point.x as f32 * GRID_SIZE.x,
                spawn_point.y as f32 * GRID_SIZE.y,
                Z_ITEM,
            ),
            scale: Vec3::new(scale_x, scale_y, 1.0),
            ..Default::default()
        },
        Visibility::Hidden,
        RenderLayers::layer(1),
    ));

    // Light-emitting props get the Candle component + animation timer so they
    // integrate with the existing lighting infrastructure.
    if asset.light_radius.is_some() {
        entity.insert((
            crate::map::light::Candle,
            crate::map::light::AnimationTimer(Timer::from_seconds(0.2, TimerMode::Repeating)),
        ));
    }

    if asset.is_blocking {
        entity.insert(Collider);
    }

    if prop_name == "chest" {
        entity.insert(crate::components::Chest);
    }

    Some(entity.id())
}
