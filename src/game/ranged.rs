use bevy::prelude::*;
use bracket_lib::prelude::{DistanceAlg, Point};

use crate::{
    components::{Name, Position, Viewshed},
    constants::BASE_ACTION_COST,
    game::{
        actions::{ActionFinishedEvent, RangedAttackIntent},
        combat::AttackIntentMessage,
        items::{Equipment, ItemProperties},
    },
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
    mut intents: MessageReader<RangedAttackIntent>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
    mut attack_writer: MessageWriter<AttackIntentMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
    attacker_query: Query<(
        &Position,
        &Viewshed,
        Option<&RangedCapable>,
        Option<&Equipment>,
        Option<&Name>,
    )>,
    target_query: Query<(&Position, Option<&Name>)>,
    item_props_query: Query<&ItemProperties>,
) {
    for intent in intents.read() {
        let Ok((attacker_pos, viewshed, ranged_capable, equipment, attacker_name)) =
            attacker_query.get(intent.attacker)
        else {
            finish_writer.write(ActionFinishedEvent {
                entity: intent.attacker,
                base_cost: BASE_ACTION_COST,
            });
            continue;
        };

        let Ok((target_pos, target_name)) = target_query.get(intent.target) else {
            finish_writer.write(ActionFinishedEvent {
                entity: intent.attacker,
                base_cost: BASE_ACTION_COST,
            });
            continue;
        };

        let target_point = Point::new(target_pos.x, target_pos.y);

        // 1. LOS check.
        if !viewshed.visible_tiles.contains(&target_point) {
            log_writer.write(GameLogMessage(
                "No clear line of sight to target.".to_string(),
            ));
            finish_writer.write(ActionFinishedEvent {
                entity: intent.attacker,
                base_cost: BASE_ACTION_COST,
            });
            continue;
        }

        // 2. Range check — monster uses RangedCapable, player uses equipped weapon.
        let range = if let Some(rc) = ranged_capable {
            rc.range
        } else if let Some(eq) = equipment {
            eq.weapon
                .and_then(|w| item_props_query.get(w).ok())
                .map(|p| p.weapon_range)
                .unwrap_or(1)
        } else {
            1
        };

        let attacker_point = Point::new(attacker_pos.x, attacker_pos.y);
        let dist = DistanceAlg::Pythagoras.distance2d(attacker_point, target_point);
        if dist > range as f32 {
            let who = attacker_name.map(|n| n.0.as_str()).unwrap_or("Target");
            log_writer.write(GameLogMessage(format!("{} is out of range.", who)));
            finish_writer.write(ActionFinishedEvent {
                entity: intent.attacker,
                base_cost: BASE_ACTION_COST,
            });
            continue;
        }

        // Log the shot.
        let attacker_str = attacker_name.map(|n| n.0.as_str()).unwrap_or("Someone");
        let target_str = target_name.map(|n| n.0.as_str()).unwrap_or("the target");
        log_writer.write(GameLogMessage(format!("{} fires at {}!", attacker_str, target_str)));

        // Valid shot — hand off to the normal attack pipeline.
        attack_writer.write(AttackIntentMessage {
            attacker: intent.attacker,
            target: intent.target,
        });
        finish_writer.write(ActionFinishedEvent {
            entity: intent.attacker,
            base_cost: BASE_ACTION_COST,
        });
    }
}
