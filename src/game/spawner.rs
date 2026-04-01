use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::prelude::Text2d;
use bevy::text::{TextFont, TextColor};
use bracket_lib::prelude::Point;

use crate::{
    assets::{
        AbilityDef, MonsterAsset, MonsterManifest, MonsterManifestHandle, MonsterSpriteAssets,
        ItemManifest, ItemManifestHandle, ItemSpriteAssets,
        PropManifest, PropManifestHandle, PropSpriteAssets,
    },
    components::{
        Ammo, Collider, Faction, FactionKind, FloorEntityMarker, GameEntityMarker, Inventory, Monster, Name, Position, Prop, Viewshed, Item,
    },
    constants::{TILE_SIZE_X, TILE_SIZE_Y, Z_MONSTER, Z_ITEM},
    game::{
        MonsterAI, TurnManager,
        actions::SpeedStats,
        combat::{Damage, DamageType, DamageTypeTag, Health, HealthRegen, Resistances},
        items::{ItemProperties, ItemStack, LootEntry, LootTable},
        magic::StatusEffects,
        ranged::RangedCapable,
        staves::MonsterAbilities,
        stats::{Armor, DamageBonus, Dodge, HitBonus},
    },
    map::map::GRID_SIZE,
};

/// Attach an AsciiGlyph child entity to a parent for ASCII rendering mode.
/// The glyph starts hidden and becomes visible when GraphicsMode switches to Ascii.
/// `parent_scale` is the parent entity's transform scale — the glyph counter-scales
/// so text renders at the correct pixel size regardless of parent sprite scaling.
pub fn attach_ascii_glyph(
    commands: &mut Commands,
    parent: Entity,
    ascii_char: &str,
    ascii_fg: Color,
    font: &Handle<Font>,
    parent_scale: Vec3,
) {
    let display = if ascii_char.is_empty() { "?" } else { ascii_char };
    let inv_scale = Vec3::new(1.0 / parent_scale.x, 1.0 / parent_scale.y, 1.0);
    // Use Inherited so the glyph follows the parent's visibility.
    // In ASCII mode the parent visibility system controls when items are
    // shown/hidden; in Sprite mode apply_graphics_mode_swap sets all
    // AsciiGlyph entities to Hidden on mode change.
    let glyph = commands
        .spawn((
            Text2d::new(display.to_string()),
            TextFont {
                font: font.clone(),
                font_size: 14.0,
                ..default()
            },
            TextColor(ascii_fg),
            Transform {
                scale: inv_scale,
                ..default()
            },
            Visibility::Inherited,
            crate::game::ascii_mode::AsciiGlyph,
            RenderLayers::layer(1),
        ))
        .id();
    commands.entity(parent).add_child(glyph);
}

pub fn spawn_monster(
    commands: &mut Commands,
    spawn_point: &Point,
    turn_manager: &mut ResMut<TurnManager>,
    monster_asset: &MonsterAsset,
    monster_sprite_assets: &Res<MonsterSpriteAssets>,
    ascii_font: Option<&crate::game::ascii_mode::AsciiFont>,
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

    let spawn_pt = Point::new(spawn_point.x, spawn_point.y);
    let (mut monster_ai, base_morale) = match &monster_asset.ai {
        crate::assets::AiConfig::Fsm { flee_at_hp_percent, erratic_chance, chase_leash, kites, kite_distance, .. } => {
            let mut ai = MonsterAI::default();
            ai.flee_at_hp_percent = *flee_at_hp_percent;
            ai.erratic_chance = *erratic_chance;
            ai.chase_leash = *chase_leash;
            ai.kites = *kites;
            ai.kite_distance = *kite_distance;
            (ai, 0.6) // Default morale for FSM monsters
        }
        crate::assets::AiConfig::Goap { base_morale, .. } => {
            (MonsterAI::default(), *base_morale)
        }
    };
    monster_ai.spawn_position = Some(spawn_pt);
    monster_ai.stationary = monster_asset.stationary;

    let monster_entity = commands
        .spawn((
            Monster,
            GameEntityMarker,
            FloorEntityMarker,
            Name(monster_asset.name.clone()),
            monster_ai,
            Collider,
            new_grid_pos,
            new_pos,
            Viewshed::new(monster_asset.vision.max(2)),
            Faction(FactionKind(monster_asset.faction.clone())),
            StatusEffects::default(),
            Inventory { items: vec![], capacity: 20 },
            crate::game::squad::Morale::new(base_morale),
        ))
        .insert((
            Health {
                current: monster_asset.base_hp,
                max: monster_asset.base_hp,
            },
            Damage(monster_asset.damage.clone()),
            SpeedStats::new(monster_asset.movement_delay, monster_asset.attack_delay),
            Armor(monster_asset.base_armor),
            Dodge(monster_asset.base_dodge),
            HitBonus(0),
            DamageBonus(0),
            monster_asset.movement_mode,
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

    // Monster abilities (cooldown-based spell replacement)
    if !monster_asset.monster_abilities.is_empty() {
        commands.entity(monster_entity).insert(MonsterAbilities(monster_asset.monster_abilities.clone()));
    }

    let ranged_range = match &monster_asset.ai {
        crate::assets::AiConfig::Fsm { ranged_range, .. } => *ranged_range,
        crate::assets::AiConfig::Goap { traits, .. } => {
            traits.iter().find_map(|t| match t {
                crate::assets::AiTrait::Ranged { range } => Some(*range),
                _ => None,
            }).unwrap_or(0)
        }
    };
    if ranged_range > 0 {
        commands
            .entity(monster_entity)
            .insert(RangedCapable { range: ranged_range });
    }

    // Damage type tag (for melee attacks)
    let dmg_type = DamageType::from_str(&monster_asset.damage_type);
    if dmg_type != DamageType::Physical {
        commands.entity(monster_entity).insert(DamageTypeTag(dmg_type));
    }

    // Resistances
    if !monster_asset.resistances.is_empty() {
        let mut map = std::collections::HashMap::new();
        for (dt_str, val) in &monster_asset.resistances {
            map.insert(DamageType::from_str(dt_str), *val);
        }
        commands.entity(monster_entity).insert(Resistances(map));
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
            AbilityDef::ExplodeOnDeath { damage, radius, damage_type } => {
                commands.entity(monster_entity).insert(ExplodeOnDeath {
                    damage: *damage,
                    radius: *radius,
                    damage_type: damage_type.as_ref().map(|s| DamageType::from_str(s)).unwrap_or(DamageType::Fire),
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
            AbilityDef::SplitOnHit { min_hp } => {
                commands.entity(monster_entity).insert(SplitOnHit {
                    min_hp: *min_hp,
                });
            }
            AbilityDef::MimicDisguise => {
                commands.entity(monster_entity).insert(MimicDisguise);
            }
        }
    }

    // ASCII glyph child
    if let Some(font) = ascii_font {
        attach_ascii_glyph(commands, monster_entity, &monster_asset.ascii_char, monster_asset.ascii_fg, &font.0, Vec3::new(scale_x, scale_y, 1.0));
    }

    // GOAP monsters get a GoapAI component with trait-driven goals/actions.
    if let crate::assets::AiConfig::Goap { traits, .. } = &monster_asset.ai {
        let (goals, actions) = crate::game::goap::build_goap_config(
            traits,
            !monster_asset.monster_abilities.is_empty(),
            monster_asset.base_armor >= 2,
            /* is_squad_member */ false, // Will be set later when squad is assigned
        );
        commands.entity(monster_entity).insert(crate::game::goap::GoapAI {
            goals,
            actions,
            hoard_position: if traits.iter().any(|t| matches!(t, crate::assets::AiTrait::Hoarder)) {
                Some(spawn_pt)
            } else {
                None
            },
            roam_target: None,
        });
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
    ascii_font: Option<&crate::game::ascii_mode::AsciiFont>,
) -> Option<Entity> {
    if let Some(manifest) = monster_manifests.get(&monster_manifest_handle.0) {
        if let Some(monster_asset) = manifest.monsters.get(monster_name) {
            spawn_monster(
                commands,
                spawn_point,
                turn_manager,
                monster_asset,
                monster_sprite_assets,
                ascii_font,
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

/// Spawn an item entity from the manifest.
/// If `enchant_floor_depth` is `Some(depth)`, roll random enchantment and runic for weapons/armor.
pub fn spawn_item(
    commands: &mut Commands,
    item_name: &str,
    spawn_point: &Point,
    item_manifests: &Res<Assets<ItemManifest>>,
    item_manifest_handle: &Res<ItemManifestHandle>,
    item_sprite_assets: &Res<ItemSpriteAssets>,
    ascii_font: Option<&crate::game::ascii_mode::AsciiFont>,
    enchant_floor_depth: Option<u32>,
) -> Option<Entity> {
    let manifest = item_manifests.get(&item_manifest_handle.0)?;
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
        attack_speed: asset.attack_speed,
        staff_effect: asset.staff_effect,
        base_recharge: asset.base_recharge,
        dodge_bonus: asset.dodge_bonus,
        hit_bonus: asset.hit_bonus,
        damage_bonus: asset.damage_bonus,
        regen_bonus: asset.regen_bonus,
        max_hp_bonus: asset.max_hp_bonus,
        delay_modifier: asset.delay_modifier,
        weapon_ability: asset.weapon_ability.clone(),
    });

    entity.insert(ItemStack { count: 1, max_stack: asset.max_stack });

    if asset.is_ammo {
        entity.insert(Ammo);
    }

    if asset.is_quest_item {
        entity.insert(crate::components::QuestItem);
    }

    let item_entity = entity.id();

    // ASCII glyph child
    if let Some(font) = ascii_font {
        attach_ascii_glyph(commands, item_entity, &asset.ascii_char, asset.ascii_fg, &font.0, Vec3::new(scale_x, scale_y, 1.0));
    }

    // Roll random enchantment and runic for weapons/armor
    if let Some(depth) = enchant_floor_depth {
        let mut rng = bracket_lib::random::RandomNumberGenerator::new();
        crate::game::enchantment::enchant_item(commands, item_entity, &asset.item_kind, depth, &mut rng);
    }

    // Insert staff components if this is a staff item
    if let Some(effect) = asset.staff_effect {
        let enchant_level = 0; // Staff enchantment is handled by the Enchantment component
        commands.entity(item_entity).insert((
            crate::game::staves::StaffData {
                effect,
                base_recharge: asset.base_recharge,
            },
            crate::game::staves::Rechargeable::new(asset.base_recharge, enchant_level),
        ));
    }

    Some(item_entity)
}

pub fn spawn_prop(
    commands: &mut Commands,
    prop_name: &str,
    spawn_point: &Point,
    prop_manifests: &Res<Assets<PropManifest>>,
    prop_manifest_handle: &Res<PropManifestHandle>,
    prop_sprite_assets: &Res<PropSpriteAssets>,
    ascii_font: Option<&crate::game::ascii_mode::AsciiFont>,
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
        crate::components::PropKey(prop_name.to_string()),
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

    // Barricades are destructible props that can be attacked and destroyed.
    if prop_name == "barricade" {
        entity.insert((
            Health { current: 10, max: 10 },
            Damage("1d1".to_string()),
            crate::components::Destructible,
        ));
    }

    if prop_name == "chest" {
        entity.insert(crate::components::Chest);
    }

    let prop_entity = entity.id();

    // ASCII glyph child
    if let Some(font) = ascii_font {
        attach_ascii_glyph(commands, prop_entity, &asset.ascii_char, asset.ascii_fg, &font.0, Vec3::new(scale_x, scale_y, 1.0));
    }

    Some(prop_entity)
}

/// Spawn a key item entity at the given position. The key opens a locked door
/// whose `LockedDoorData.key_name` matches `key_name`.
pub fn spawn_key(
    commands: &mut Commands,
    key_name: &str,
    position: &Point,
    ascii_font: Option<&crate::game::ascii_mode::AsciiFont>,
) -> Entity {
    use crate::components::Key;

    let scale_x = GRID_SIZE.x / 16.0;
    let scale_y = GRID_SIZE.y / 16.0;

    let entity = commands
        .spawn((
            Item,
            Name(key_name.to_string()),
            Key { key_name: key_name.to_string() },
            GameEntityMarker,
            FloorEntityMarker,
            Position { x: position.x, y: position.y },
            Sprite {
                color: Color::srgb(1.0, 0.85, 0.0), // yellow
                custom_size: Some(GRID_SIZE),
                ..default()
            },
            Transform {
                translation: Vec3::new(
                    position.x as f32 * GRID_SIZE.x,
                    position.y as f32 * GRID_SIZE.y,
                    Z_ITEM,
                ),
                scale: Vec3::new(scale_x, scale_y, 1.0),
                ..Default::default()
            },
            Visibility::Hidden,
            RenderLayers::layer(1),
        ))
        .id();

    // ASCII glyph child — render as "k" in yellow
    if let Some(font) = ascii_font {
        attach_ascii_glyph(
            commands,
            entity,
            "k",
            Color::srgb(1.0, 0.85, 0.0),
            &font.0,
            Vec3::new(scale_x, scale_y, 1.0),
        );
    }

    entity
}

