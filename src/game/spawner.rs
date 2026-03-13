use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bracket_lib::prelude::Point;

use crate::{
    assets::{MonsterAsset, MonsterManifest, MonsterManifestHandle, MonsterSpriteAssets, ItemManifest, ItemManifestHandle, ItemSpriteAssets},
    components::{
        Collider, FloorEntityMarker, GameEntityMarker, Monster, Name, Position, Viewshed, Item, AmuletOfBevy,
    },
    constants::{TILE_SIZE_X, TILE_SIZE_Y, Z_MONSTER, Z_ITEM},
    game::{
        MonsterAI, TurnManager,
        abilities::{BaseArmor, Cowardly, Faction, FactionKind, OnHitEffects},
        actions::SpeedStats,
        combat::{Damage, DamageType, DamageTypeTag, Health, HealthRegen, Resistances, ResistanceLevel},
        items::{ItemProperties, ItemStack, LootEntry, LootTable},
        level::ExperienceReward,
        magic::{ActiveSpells, KnownSpells, ManaRegen, SpellCooldowns, MAX_SPELL_SLOTS},
        ranged::RangedCapable,
        stats::{AttributeModifiers, Attributes, CombatStats, Level, Mana, MonsterBaseHealth},
    },
    map::map::GRID_SIZE,
};

pub fn spawn_monster(
    commands: &mut Commands,
    spawn_point: &Point,
    turn_manager: &mut ResMut<TurnManager>,
    monster_asset: &MonsterAsset,
    monster_sprite_assets: &Res<MonsterSpriteAssets>,
) -> Entity {
    let tile_size = monster_asset.tile_size.unwrap_or(UVec2::new(32, 32));
    let scale_x = TILE_SIZE_X as f32 / tile_size.x as f32;
    let scale_y = TILE_SIZE_Y as f32 / tile_size.y as f32;

    let new_pos = Transform {
        translation: Vec3::new(
            spawn_point.x as f32 * GRID_SIZE.x,
            spawn_point.y as f32 * GRID_SIZE.y,
            Z_MONSTER,
        ),
        scale: Vec3::new(scale_x, scale_y, 1.0), // Use calculated scale
        ..Default::default()
    };
    let new_grid_pos = Position {
        x: spawn_point.x,
        y: spawn_point.y,
    };

    let sprite_path_parts: Vec<&str> = monster_asset.sprite.split('#').collect();
    let texture_path = sprite_path_parts[0];
    let index = sprite_path_parts[1].parse::<usize>().unwrap_or_default();

    let texture_handle = monster_sprite_assets
        .handles
        .get(texture_path)
        .unwrap()
        .clone();
    let layout_handle = monster_sprite_assets
        .layouts
        .get(texture_path)
        .unwrap()
        .clone();

    // Calculate XP reward: Base 10 + (Level * 5) + (Base HP / 2)
    let xp_reward = 10 + (monster_asset.level * 5) + (monster_asset.base_hp / 2);

    // Use multiple insert calls to avoid large tuple bundle limit (15)
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
            Viewshed::new(8), // Initial range; recalculated by stat_recalculation_system via PER
            Faction(FactionKind::Monster),
        ))
        .insert((
            Health {
                current: 10, // Initial value, recalculated by stats system
                max: 10,
            },
            Damage(monster_asset.damage.clone()),
            SpeedStats::default(),
            Attributes {
                strength: monster_asset.strength,
                dexterity: monster_asset.dexterity,
                constitution: monster_asset.constitution,
                agility: monster_asset.agility,
                intelligence: monster_asset.intelligence,
                perception: monster_asset.perception,
            },
            AttributeModifiers::default(),
            Level {
                value: monster_asset.level,
            },
            MonsterBaseHealth {
                value: monster_asset.base_hp,
            },
            CombatStats::default(),
            ExperienceReward(xp_reward),
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
    if !monster_asset.spells.is_empty() || monster_asset.intelligence > 0 {
        let mana_max = monster_asset.intelligence * 5;
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

    // Base armor
    if monster_asset.base_armor > 0 {
        commands.entity(monster_entity).insert(BaseArmor(monster_asset.base_armor));
    }

    // Cowardly flee behavior
    if monster_asset.is_cowardly {
        commands.entity(monster_entity).insert(Cowardly);
    }

    // On-hit effects
    if !monster_asset.on_hit_effects.is_empty() {
        commands.entity(monster_entity).insert(OnHitEffects(monster_asset.on_hit_effects.clone()));
    }

    turn_manager.add_entity(monster_entity);
    monster_entity
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
            Some(spawn_monster(
                commands,
                spawn_point,
                turn_manager,
                monster_asset,
                monster_sprite_assets,
            ))
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

    let sprite_path_parts: Vec<&str> = asset.sprite.split('#').collect();
    let texture_path = sprite_path_parts[0];
    let index = sprite_path_parts[1].parse::<usize>().unwrap_or_default();

    let texture_handle = item_sprite_assets.handles.get(texture_path).unwrap().clone();
    let layout_handle = item_sprite_assets.layouts.get(texture_path).unwrap().clone();

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
        str_bonus: asset.str_bonus,
        dex_bonus: asset.dex_bonus,
        con_bonus: asset.con_bonus,
        agi_bonus: asset.agi_bonus,
        int_bonus: asset.int_bonus,
        per_bonus: asset.per_bonus,
        effect: asset.effect.clone(),
        weapon_range: asset.weapon_range,
    });

    entity.insert(ItemStack { count: 1, max_stack: asset.max_stack });

    if asset.is_victory {
        entity.insert(AmuletOfBevy);
    }

    Some(entity.id())
}
