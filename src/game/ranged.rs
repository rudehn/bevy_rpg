use bevy::prelude::*;
use bracket_lib::prelude::{DistanceAlg, Point};

use crate::game::turns::ProcessingPhase;

pub struct RangedPlugin;

impl Plugin for RangedPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<RangedAttackIntent>()
            .add_systems(
                Update,
                handle_ranged_attack.in_set(ProcessingPhase::ResolveActions),
            );
    }
}

use crate::{
    components::{Ammo, InInventory, Inventory, Name, Position, Submerged, Viewshed},
    constants::BASE_ACTION_COST,
    game::{
        actions::{ActionFinishedEvent, ActionKind, FreeActionEvent, RangedAttackIntent, finish_turn, free_turn},
        combat::{AttackIntentMessage, DamageType, DamageSource},
        items::{Equipment, ItemProperties, ItemStack},
        particles::ParticleRequest,
    },
    player::Player,
    ui::game_log::GameLogMessage,
};

/// Marks a monster as capable of ranged attacks with the given tile range.
#[derive(Component, Clone)]
pub struct RangedCapable {
    pub range: u32,
}

/// Validates and executes a ranged attack intent.
///
/// Checks:
/// 1. LOS — target must be in the attacker's viewshed.
/// 2. Range — determined by `RangedCapable` (monsters) or equipped weapon's `weapon_range` (player).
///
/// On success emits `AttackIntentMessage` into the normal combat pipeline.
/// On failure logs the reason and still emits `ActionFinishedEvent` so the turn advances.
pub fn handle_ranged_attack(
    mut commands: Commands,
    mut intents: MessageReader<RangedAttackIntent>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
    mut free_writer: MessageWriter<FreeActionEvent>,
    mut attack_writer: MessageWriter<AttackIntentMessage>,
    mut particle_writer: MessageWriter<ParticleRequest>,
    mut log_writer: MessageWriter<GameLogMessage>,
    attacker_query: Query<(
        &Position,
        &Viewshed,
        Option<&RangedCapable>,
        Option<&Equipment>,
        Option<&Name>,
        Has<Player>,
    )>,
    target_query: Query<(&Position, Option<&Name>, Has<Submerged>)>,
    item_props_query: Query<&ItemProperties>,
    mut player_inv_query: Query<&mut Inventory, With<Player>>,
    arrow_query: Query<(&Name, &ItemStack), (With<InInventory>, With<Ammo>)>,
) {
    for intent in intents.read() {
        let Ok((attacker_pos, viewshed, ranged_capable, equipment, attacker_name, is_player)) =
            attacker_query.get(intent.attacker)
        else {
            finish_turn(&mut commands, &mut finish_writer, intent.attacker, BASE_ACTION_COST, ActionKind::Attack);
            continue;
        };

        let Ok((target_pos, target_name, target_submerged)) = target_query.get(intent.target) else {
            if is_player {
                free_turn(&mut commands, &mut free_writer, intent.attacker);
            } else {
                finish_turn(&mut commands, &mut finish_writer, intent.attacker, BASE_ACTION_COST, ActionKind::Attack);
            }
            continue;
        };

        // Submerged targets cannot be hit by ranged attacks.
        if target_submerged {
            log_writer.write(GameLogMessage("The target is submerged and cannot be hit!".to_string()));
            if is_player {
                free_turn(&mut commands, &mut free_writer, intent.attacker);
            } else {
                finish_turn(&mut commands, &mut finish_writer, intent.attacker, BASE_ACTION_COST, ActionKind::Attack);
            }
            continue;
        }

        let target_point = Point::new(target_pos.x, target_pos.y);

        // 1. LOS check.
        if !viewshed.visible_tiles.contains(&target_point) {
            log_writer.write(GameLogMessage(
                "No clear line of sight to target.".to_string(),
            ));
            if is_player {
                free_turn(&mut commands, &mut free_writer, intent.attacker);
            } else {
                finish_turn(&mut commands, &mut finish_writer, intent.attacker, BASE_ACTION_COST, ActionKind::Attack);
            }
            continue;
        }

        // 2. Range check — monster uses RangedCapable, player uses equipped weapon.
        //    If the player has no ranged weapon equipped, treat as invalid (free action).
        let range = if let Some(rc) = ranged_capable {
            rc.range
        } else if let Some(eq) = equipment {
            let weapon_range = eq.weapon
                .and_then(|w| item_props_query.get(w).ok())
                .map(|p| p.weapon_range)
                .unwrap_or(0);
            if is_player && weapon_range == 0 {
                log_writer.write(GameLogMessage("You have no ranged weapon equipped.".to_string()));
                free_turn(&mut commands, &mut free_writer, intent.attacker);
                continue;
            }
            weapon_range
        } else {
            if is_player {
                log_writer.write(GameLogMessage("You have no ranged weapon equipped.".to_string()));
                free_turn(&mut commands, &mut free_writer, intent.attacker);
                continue;
            }
            1
        };

        let attacker_point = Point::new(attacker_pos.x, attacker_pos.y);
        let dist = DistanceAlg::Pythagoras.distance2d(attacker_point, target_point);
        if dist > range as f32 {
            let who = attacker_name.map(|n| n.0.as_str()).unwrap_or("Target");
            log_writer.write(GameLogMessage(format!("{} is out of range.", who)));
            if is_player {
                free_turn(&mut commands, &mut free_writer, intent.attacker);
            } else {
                finish_turn(&mut commands, &mut finish_writer, intent.attacker, BASE_ACTION_COST, ActionKind::Attack);
            }
            continue;
        }

        // Player must have at least one arrow to fire.
        if is_player {
            let Ok(mut inv) = player_inv_query.single_mut() else {
                free_turn(&mut commands, &mut free_writer, intent.attacker);
                continue;
            };

            let arrow = inv.items.iter().find_map(|&e| {
                arrow_query.get(e).ok().map(|(_name, stack)| {
                    (e, stack.count, stack.max_stack)
                })
            });

            let Some((arrow_entity, arrow_count, arrow_max)) = arrow else {
                log_writer.write(GameLogMessage("You have no arrows!".to_string()));
                free_turn(&mut commands, &mut free_writer, intent.attacker);
                continue;
            };

            // Consume one arrow from the stack.
            if arrow_count > 1 {
                commands.entity(arrow_entity).insert(ItemStack { count: arrow_count - 1, max_stack: arrow_max });
            } else {
                // Last arrow: remove from inventory and despawn.
                inv.items.retain(|&e| e != arrow_entity);
                commands.entity(arrow_entity).despawn();
            }
        }

        // Log the shot.
        let attacker_str = attacker_name.map(|n| n.0.as_str()).unwrap_or("Someone");
        let target_str = target_name.map(|n| n.0.as_str()).unwrap_or("the target");
        log_writer.write(GameLogMessage(format!("{} fires at {}!", attacker_str, target_str)));

        // Arrow particle effect.
        particle_writer.write(ParticleRequest::arrow(
            (attacker_pos.x, attacker_pos.y),
            (target_pos.x, target_pos.y),
        ));

        // Valid shot — hand off to the normal attack pipeline.
        // Ranged attacks are always physical for now.
        attack_writer.write(AttackIntentMessage {
            attacker: intent.attacker,
            target: intent.target,
            damage_type: DamageType::Physical,
            source: DamageSource::Ranged,
        });
        finish_turn(&mut commands, &mut finish_writer, intent.attacker, BASE_ACTION_COST, ActionKind::Attack);
    }
}
