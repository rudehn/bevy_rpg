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

/// Attach an AsciiGlyph child entity to a parent for ASCII rendering.
/// `parent_scale` is the parent entity's transform scale — the glyph counter-scales
/// so text renders at the correct pixel size regardless of parent scaling.

/// Resolve sprite handles and scale from a sprite path and asset maps.
/// Returns `None` if the texture or layout is missing.
/// Used to determine the scale factor for ASCII glyph counter-scaling.
pub fn resolve_sprite(
    sprite_path: &str,
    default_tile_size: UVec2,
    tile_size_override: Option<UVec2>,
    handles: &std::collections::HashMap<String, Handle<Image>>,
    layouts: &std::collections::HashMap<String, Handle<bevy::prelude::TextureAtlasLayout>>,
) -> Option<(Handle<Image>, Handle<bevy::prelude::TextureAtlasLayout>, usize, f32, f32)> {
    let (texture_path, index) = crate::assets::parse_sprite_path(sprite_path);
    let texture_handle = handles.get(texture_path).cloned()?;
    let layout_handle = layouts.get(texture_path).cloned()?;
    let tile_size = tile_size_override.unwrap_or(default_tile_size);
    let scale_x = crate::map::map::GRID_SIZE.x / tile_size.x as f32;
    let scale_y = crate::map::map::GRID_SIZE.y / tile_size.y as f32;
    Some((texture_handle, layout_handle, index, scale_x, scale_y))
}

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
    // The parent visibility system controls when items are shown/hidden.
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
            crate::game::ascii_mode::AsciiGlyphColor(ascii_fg),
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
    let scale_x = GRID_SIZE.x / tile_size.x as f32;
    let scale_y = GRID_SIZE.y / tile_size.y as f32;

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
            monster_asset.species,
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
            crate::game::stats::Block(0),
            crate::game::stats::MaxShieldBlocks(0),
            crate::game::stats::ShieldBlocksUsed(0),
            Dodge(monster_asset.base_dodge),
            HitBonus(0),
            DamageBonus(0),
            monster_asset.movement_mode,
        ))
        .insert((
            Visibility::Hidden,
            RenderLayers::layer(1),
        ))
        .id();

    // Phase 2: every monster carries its tier so XP-on-kill can scale
    // and apply the anti-farming dropoff.
    commands
        .entity(monster_entity)
        .insert(crate::game::xp::MonsterTier(monster_asset.tier));

    // Phase B: monsters with declared `equipped:` get an empty Equipment
    // component immediately and an UnequippedLoadout marker for the
    // deferred-equip system to process next frame. Keeps this function's
    // signature small — the equip system pulls ItemManifest via Res.
    if !monster_asset.equipped.is_empty() {
        commands.entity(monster_entity).insert((
            crate::game::items::Equipment::default(),
            crate::game::items::UnequippedLoadout(monster_asset.equipped.clone()),
        ));
    }

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
            AbilityDef::PoisonStrike { damage_per_turn, duration, chance } => {
                commands.entity(monster_entity).insert(crate::game::abilities::PoisonStrike {
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
            AbilityDef::ExplodeOnHit { radius, effect } => {
                use crate::assets::ExplodeEffectDef;
                use crate::game::abilities::ExplodeEffect;
                let effect = match effect {
                    ExplodeEffectDef::CrackFloor => ExplodeEffect::CrackFloor,
                    ExplodeEffectDef::GasCloud { volume } => {
                        ExplodeEffect::GasCloud { volume: *volume }
                    }
                };
                commands.entity(monster_entity).insert(ExplodeOnHit {
                    radius: *radius,
                    effect,
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
            AbilityDef::GasOnDeath { radius, volume } => {
                commands.entity(monster_entity).insert(GasOnDeath {
                    radius: *radius,
                    volume: *volume,
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

    commands.entity(monster_entity).insert(crate::game::ascii_mode::AsciiDisplay {
        ch: if monster_asset.ascii_char.is_empty() { "?".to_string() } else { monster_asset.ascii_char.clone() },
        color: monster_asset.ascii_fg,
    });

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
            last_action: None,
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

    let item_tile_size = asset.tile_size.unwrap_or(UVec2::new(32, 32));
    let scale_x = GRID_SIZE.x / item_tile_size.x as f32;
    let scale_y = GRID_SIZE.y / item_tile_size.y as f32;

    let mut entity = commands.spawn((
        Item,
        Name(asset.name.clone()),
        GameEntityMarker,
        FloorEntityMarker,
        Position { x: spawn_point.x, y: spawn_point.y },
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

    // Unpack the tagged-union kind into the runtime ItemProperties shape.
    // ItemProperties stays flat — handlers across the codebase read those
    // fields directly. The asset is the only authoring shape that uses
    // the tagged union.
    let kind = asset.item_kind();
    let armor_slot = asset.armor_data().map(|a| a.slot.clone());
    let defense = asset.armor_data().map(|a| a.defense).unwrap_or(0);
    let max_blocks = asset.armor_data().map(|a| a.max_blocks).unwrap_or(0);
    let damage = asset.weapon_data().map(|w| w.damage.clone());
    let weapon_range = asset.weapon_data().map(|w| w.weapon_range).unwrap_or(0);
    let attack_speed = asset.weapon_data().map(|w| w.attack_speed).unwrap_or(1.0);
    let weapon_ability = asset.weapon_data().and_then(|w| w.weapon_ability.clone());
    let weapon_skill = asset.weapon_data().and_then(|w| w.weapon_skill);
    let on_hit_effects = asset
        .weapon_data()
        .map(|w| w.on_hit_effects.clone())
        .unwrap_or_default();
    let staff_effect = asset.staff_data().map(|s| s.effect);
    let staff_base_recharge = asset.staff_data().map(|s| s.base_recharge).unwrap_or(0);
    let consumable_effect = asset.consumable_data().and_then(|c| c.effect.clone());
    let max_stack = asset.consumable_data().map(|c| c.max_stack).unwrap_or(1);
    let is_ammo = asset.consumable_data().map(|c| c.is_ammo).unwrap_or(false);

    // Staves get Effect::ZapStaff automatically so they can be used from inventory.
    let effect = if staff_effect.is_some() && consumable_effect.is_none() {
        Some(crate::game::effects::Effect::ZapStaff)
    } else {
        consumable_effect
    };

    // Convert the manifest's string-keyed resistance map to typed
    // DamageType keys once, at spawn — handlers can iterate without
    // re-parsing strings on every equip/unequip.
    let resistances = asset
        .resistances
        .iter()
        .map(|(k, v)| (DamageType::from_str(k), *v))
        .collect();

    entity.insert(ItemProperties {
        kind: kind.clone(),
        armor_slot: armor_slot.clone(),
        damage,
        defense,
        rarity: asset.rarity.clone(),
        effect,
        weapon_range,
        attack_speed,
        staff_effect,
        base_recharge: staff_base_recharge,
        dodge_bonus: asset.dodge_bonus,
        hit_bonus: asset.hit_bonus,
        damage_bonus: asset.damage_bonus,
        regen_bonus: asset.regen_bonus,
        max_hp_bonus: asset.max_hp_bonus,
        delay_modifier: asset.delay_modifier,
        vision_bonus: asset.vision_bonus,
        resistances,
        weapon_ability,
        weapon_skill,
        max_blocks,
        on_hit_effects,
        armor_stealth_penalty: asset.armor_stealth_penalty,
    });

    entity.insert(ItemStack { count: 1, max_stack });

    // Consumable items (potions, scrolls) are destroyed on use.
    if matches!(kind, crate::game::items::ItemKind::Consumable) {
        entity.insert(crate::components::Consumable);
    }

    if is_ammo {
        entity.insert(Ammo);
    }

    if asset.is_quest_item {
        entity.insert(crate::components::QuestItem);
    }

    entity.insert(crate::game::ascii_mode::AsciiDisplay {
        ch: if asset.ascii_char.is_empty() { "?".to_string() } else { asset.ascii_char.clone() },
        color: asset.ascii_fg,
    });

    let item_entity = entity.id();

    // ASCII glyph child
    if let Some(font) = ascii_font {
        attach_ascii_glyph(commands, item_entity, &asset.ascii_char, asset.ascii_fg, &font.0, Vec3::new(scale_x, scale_y, 1.0));
    }

    // Roll random enchantment and runic for weapons/armor
    if let Some(depth) = enchant_floor_depth {
        let mut rng = bracket_lib::random::RandomNumberGenerator::new();
        crate::game::enchantment::enchant_item(commands, item_entity, &kind, depth, &mut rng);
    }

    // Insert staff components if this is a staff item
    if let Some(effect) = staff_effect {
        let enchant_level = 0; // Staff enchantment is handled by the Enchantment component
        commands.entity(item_entity).insert((
            crate::game::staves::StaffData {
                effect,
                base_recharge: staff_base_recharge,
            },
            crate::game::staves::Rechargeable::new(staff_base_recharge, enchant_level),
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

    let prop_tile_size = asset.tile_size.unwrap_or(UVec2::new(16, 16));
    let scale_x = GRID_SIZE.x / prop_tile_size.x as f32;
    let scale_y = GRID_SIZE.y / prop_tile_size.y as f32;

    let mut entity = commands.spawn((
        Prop,
        crate::components::PropKey(prop_name.to_string()),
        Name(asset.name.clone()),
        GameEntityMarker,
        FloorEntityMarker,
        Position { x: spawn_point.x, y: spawn_point.y },
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
    if let Some(radius) = asset.light_radius {
        let color = asset.light_color
            .map(|[r, g, b]| Color::srgb(r, g, b))
            .unwrap_or(Color::srgb(1.0, 0.9, 0.6));
        entity.insert((
            crate::map::light::LightSource {
                radius,
                intensity: 1.0,
                color,
                on_wall: true,
            },
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

    entity.insert(crate::game::ascii_mode::AsciiDisplay {
        ch: if asset.ascii_char.is_empty() { "?".to_string() } else { asset.ascii_char.clone() },
        color: asset.ascii_fg,
    });

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

