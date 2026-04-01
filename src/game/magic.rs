use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    components::{Name, Position},
    game::{
        combat::{ApplyDamageMessage, DamageSource, DamageType, GameRng},
        turns::TurnEndEvent,
        AppState,
    },
    map::Map,
    ui::game_log::GameLogMessage,
};

// =====================================================================
// Unified Status Effects
// =====================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum StatusEffectKind {
    Hasted,
    Slowed,
    Stunned,
    Entangled,
    Burning { damage_per_turn: i32 },
    Poisoned { damage_per_turn: i32 },
    Enraged,
    FireResistance,
    PoisonResistance,
}

impl StatusEffectKind {
    pub fn name(&self) -> &str {
        match self {
            Self::Hasted => "Hasted",
            Self::Slowed => "Slowed",
            Self::Stunned => "Stunned",
            Self::Entangled => "Entangled",
            Self::Burning { .. } => "Burning",
            Self::Poisoned { .. } => "Poisoned",
            Self::Enraged => "Enraged",
            Self::FireResistance => "Fire Resistance",
            Self::PoisonResistance => "Poison Resistance",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            Self::Hasted => Color::srgb(1.0, 1.0, 0.3),
            Self::Slowed => Color::srgb(0.5, 0.5, 0.9),
            Self::Stunned => Color::srgb(1.0, 1.0, 0.0),
            Self::Entangled => Color::srgb(0.8, 0.8, 0.8),
            Self::Burning { .. } => Color::srgb(1.0, 0.5, 0.1),
            Self::Poisoned { .. } => Color::srgb(0.3, 0.9, 0.3),
            Self::Enraged => Color::srgb(0.9, 0.2, 0.2),
            Self::FireResistance => Color::srgb(1.0, 0.6, 0.2),
            Self::PoisonResistance => Color::srgb(0.4, 1.0, 0.4),
        }
    }

    fn same_kind(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Reflect)]
pub struct ActiveStatusEffect {
    pub kind: StatusEffectKind,
    pub turns_remaining: u32,
}

/// Unified container for all status effects on an entity.
#[derive(Component, Debug, Clone, Default, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct StatusEffects(pub Vec<ActiveStatusEffect>);

impl StatusEffects {
    /// Add or refresh a status effect. If the same kind already exists, takes the longer duration.
    pub fn add(&mut self, kind: StatusEffectKind, turns: u32) {
        if let Some(existing) = self.0.iter_mut().find(|e| e.kind.same_kind(&kind)) {
            existing.turns_remaining = existing.turns_remaining.max(turns);
            // For DoT effects, take the higher damage_per_turn
            match (&mut existing.kind, &kind) {
                (StatusEffectKind::Burning { damage_per_turn: old }, StatusEffectKind::Burning { damage_per_turn: new }) => {
                    *old = (*old).max(*new);
                }
                (StatusEffectKind::Poisoned { damage_per_turn: old }, StatusEffectKind::Poisoned { damage_per_turn: new }) => {
                    *old = (*old).max(*new);
                }
                _ => {}
            }
        } else {
            self.0.push(ActiveStatusEffect { kind, turns_remaining: turns });
        }
    }

    pub fn remove_kind(&mut self, matcher: impl Fn(&StatusEffectKind) -> bool) {
        self.0.retain(|e| !matcher(&e.kind));
    }

    pub fn is_stunned(&self) -> bool {
        self.0.iter().any(|e| matches!(e.kind, StatusEffectKind::Stunned))
    }

    pub fn is_entangled(&self) -> bool {
        self.0.iter().any(|e| matches!(e.kind, StatusEffectKind::Entangled))
    }

    pub fn is_hasted(&self) -> bool {
        self.0.iter().any(|e| matches!(e.kind, StatusEffectKind::Hasted))
    }

    pub fn is_slowed(&self) -> bool {
        self.0.iter().any(|e| matches!(e.kind, StatusEffectKind::Slowed))
    }

    pub fn is_enraged(&self) -> bool {
        self.0.iter().any(|e| matches!(e.kind, StatusEffectKind::Enraged))
    }

    pub fn is_poisoned(&self) -> bool {
        self.0.iter().any(|e| matches!(e.kind, StatusEffectKind::Poisoned { .. }))
    }

    pub fn burning_damage(&self) -> Option<i32> {
        self.0.iter().find_map(|e| match e.kind {
            StatusEffectKind::Burning { damage_per_turn } => Some(damage_per_turn),
            _ => None,
        })
    }

    pub fn poison_damage(&self) -> Option<i32> {
        self.0.iter().find_map(|e| match e.kind {
            StatusEffectKind::Poisoned { damage_per_turn } => Some(damage_per_turn),
            _ => None,
        })
    }

    pub fn speed_delay_multiplier(&self) -> f32 {
        let mut delay = 1.0f32;
        if self.is_hasted() { delay *= 0.5; }
        if self.is_slowed() { delay *= 1.5; }
        delay.clamp(0.5, 2.0)
    }

    /// Tick all effects, decrementing turns_remaining. Returns expired effects.
    pub fn tick_all(&mut self) -> Vec<StatusEffectKind> {
        let mut expired = Vec::new();
        self.0.retain_mut(|effect| {
            effect.turns_remaining = effect.turns_remaining.saturating_sub(1);
            if effect.turns_remaining == 0 {
                expired.push(effect.kind);
                false
            } else {
                true
            }
        });
        expired
    }

    /// Returns display entries for UI rendering: (name, color) pairs.
    pub fn display_entries(&self) -> Vec<(&str, Color)> {
        self.0.iter().map(|e| (e.kind.name(), e.kind.color())).collect()
    }
}

// =====================================================================
// Tick Systems (run on TurnEndEvent)
// =====================================================================

/// Unified tick system for all status effects via `StatusEffects` component.
pub fn tick_status_effects_system(
    mut turn_end: MessageReader<TurnEndEvent>,
    mut query: Query<(Entity, &mut StatusEffects, &Name, &crate::components::Position)>,
    mut damage_writer: MessageWriter<ApplyDamageMessage>,
    mut decoration_mutation_writer: MessageWriter<crate::map::tile::DecorationMutationMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for _ in turn_end.read() {
        for (entity, mut effects, name, pos) in query.iter_mut() {
            // Process burning damage before ticking
            if let Some(dmg) = effects.burning_damage() {
                log_writer.write(GameLogMessage(format!(
                    "{} takes {} fire damage from burning!",
                    name.0, dmg
                )));
                damage_writer.write(ApplyDamageMessage {
                    attacker: entity,
                    target: entity,
                    final_damage: dmg,
                    damage_type: DamageType::Fire,
                    source: DamageSource::Environment,
                });
            }

            // Process poison damage before ticking
            if let Some(dmg) = effects.poison_damage() {
                log_writer.write(GameLogMessage(format!(
                    "{} takes {} poison damage!",
                    name.0, dmg
                )));
                damage_writer.write(ApplyDamageMessage {
                    attacker: entity,
                    target: entity,
                    final_damage: dmg,
                    damage_type: DamageType::Poison,
                    source: DamageSource::Environment,
                });
            }

            let expired = effects.tick_all();
            for kind in expired {
                match kind {
                    StatusEffectKind::Stunned => {
                        log_writer.write(GameLogMessage(format!(
                            "{} is no longer stunned.",
                            name.0
                        )));
                    }
                    StatusEffectKind::Entangled => {
                        log_writer.write(GameLogMessage(format!(
                            "{} breaks free of the cobwebs!",
                            name.0
                        )));
                        // Remove the cobweb at this entity's position
                        decoration_mutation_writer.write(
                            crate::map::tile::DecorationMutationMessage {
                                position: bracket_lib::prelude::Point::new(pos.x, pos.y),
                                new_decoration: crate::map::tile::Decoration::None,
                            },
                        );
                    }
                    StatusEffectKind::Burning { .. } => {
                        log_writer.write(GameLogMessage(format!(
                            "{} is no longer burning.",
                            name.0
                        )));
                    }
                    StatusEffectKind::Poisoned { .. } => {
                        log_writer.write(GameLogMessage(format!(
                            "{} is no longer poisoned.",
                            name.0
                        )));
                    }
                    StatusEffectKind::FireResistance => {
                        log_writer.write(GameLogMessage(format!(
                            "{}'s fire resistance fades.",
                            name.0
                        )));
                    }
                    StatusEffectKind::PoisonResistance => {
                        log_writer.write(GameLogMessage(format!(
                            "{}'s poison resistance fades.",
                            name.0
                        )));
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Apply speed multipliers from unified StatusEffects.
/// Recomputes both movement and attack delays each frame so that the monster's innate
/// speed is preserved while temporary buffs/debuffs layer on top.
pub fn apply_speed_effects_system(
    mut query: Query<(&mut crate::game::actions::SpeedStats, &StatusEffects)>,
) {
    for (mut speed, effects) in query.iter_mut() {
        let multiplier = effects.speed_delay_multiplier();
        speed.movement_delay = speed.base_movement_delay * multiplier;
        speed.attack_delay = speed.base_attack_delay * multiplier;
    }
}

// =====================================================================
// Pending Summon — used by monster abilities for summoning
// =====================================================================

/// Resource written by ability handlers, consumed by process_pending_summon.
#[derive(Resource)]
pub struct PendingSummon {
    pub caster_pos: Position,
    pub caster_label: String,
    pub monster_name: String,
    pub count: u32,
}

pub fn process_pending_summon(
    mut commands: Commands,
    pending: Option<Res<PendingSummon>>,
    mut turn_manager: ResMut<crate::game::TurnManager>,
    monster_manifests: Res<Assets<crate::assets::MonsterManifest>>,
    monster_manifest_handle: Res<crate::assets::MonsterManifestHandle>,
    monster_sprite_assets: Res<crate::assets::MonsterSpriteAssets>,
    map: Res<Map>,
    mut log_writer: MessageWriter<GameLogMessage>,
    positions: Query<&Position>,
) {
    let Some(summon) = pending else { return; };

    let occupied: std::collections::HashSet<(i32, i32)> = positions
        .iter()
        .map(|p| (p.x, p.y))
        .collect();

    let directions = [(0, -1), (0, 1), (-1, 0), (1, 0), (-1, -1), (1, -1), (-1, 1), (1, 1)];
    let mut spawn_points = Vec::new();
    for (dx, dy) in &directions {
        let nx = summon.caster_pos.x + dx;
        let ny = summon.caster_pos.y + dy;
        let idx = map.xy_idx(nx, ny);
        if idx < map.tiles.len()
            && crate::map::tile::is_walkable(map.tiles[idx])
            && !occupied.contains(&(nx, ny))
        {
            spawn_points.push(bracket_lib::prelude::Point::new(nx, ny));
            if spawn_points.len() >= summon.count as usize {
                break;
            }
        }
    }

    if !spawn_points.is_empty() {
        let spawned = spawn_points.len();
        for point in spawn_points {
            crate::game::spawner::spawn_monster_by_name(
                &mut commands,
                &summon.monster_name,
                &point,
                &mut turn_manager,
                &monster_manifests,
                &monster_manifest_handle,
                &monster_sprite_assets,
                None,
            );
        }
        log_writer.write(GameLogMessage(format!(
            "{} raises {} {}!",
            summon.caster_label, spawned, summon.monster_name
        )));
    }

    commands.remove_resource::<PendingSummon>();
}

// =====================================================================
// Plugin
// =====================================================================

pub struct MagicPlugin;

impl Plugin for MagicPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<StatusEffects>()
            .register_type::<StatusEffectKind>()
            .register_type::<ActiveStatusEffect>()
            .add_systems(
                Update,
                (
                    tick_status_effects_system,
                    apply_speed_effects_system,
                    process_pending_summon,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_burning_effect() {
        let mut effects = StatusEffects::default();
        effects.add(StatusEffectKind::Burning { damage_per_turn: 3 }, 5);
        assert!(effects.burning_damage().is_some());
        assert_eq!(effects.burning_damage().unwrap(), 3);
    }

    #[test]
    fn remove_burning_via_remove_kind() {
        let mut effects = StatusEffects::default();
        effects.add(StatusEffectKind::Burning { damage_per_turn: 3 }, 5);
        effects.add(StatusEffectKind::Poisoned { damage_per_turn: 2 }, 3);
        assert!(effects.burning_damage().is_some());
        assert!(effects.poison_damage().is_some());

        // Remove burning (same pattern used by water_extinguish_system)
        effects.remove_kind(|k| matches!(k, StatusEffectKind::Burning { .. }));

        assert!(effects.burning_damage().is_none());
        // Poison should still be present
        assert!(effects.poison_damage().is_some());
    }

    #[test]
    fn remove_burning_when_none_is_noop() {
        let mut effects = StatusEffects::default();
        effects.add(StatusEffectKind::Poisoned { damage_per_turn: 2 }, 3);
        // Removing burning when none exists should not panic or affect other effects
        effects.remove_kind(|k| matches!(k, StatusEffectKind::Burning { .. }));
        assert!(effects.poison_damage().is_some());
    }

    #[test]
    fn tick_decrements_and_expires() {
        let mut effects = StatusEffects::default();
        effects.add(StatusEffectKind::Burning { damage_per_turn: 3 }, 2);
        assert!(effects.burning_damage().is_some());

        let expired = effects.tick_all();
        assert!(expired.is_empty()); // 1 turn left
        assert!(effects.burning_damage().is_some());

        let expired = effects.tick_all();
        assert_eq!(expired.len(), 1); // Expired
        assert!(effects.burning_damage().is_none());
    }

    #[test]
    fn refresh_takes_longer_duration() {
        let mut effects = StatusEffects::default();
        effects.add(StatusEffectKind::Burning { damage_per_turn: 3 }, 2);
        effects.add(StatusEffectKind::Burning { damage_per_turn: 5 }, 10);
        // Duration should be max(2, 10) = 10, damage should be max(3, 5) = 5
        assert_eq!(effects.burning_damage().unwrap(), 5);
        assert_eq!(effects.0.len(), 1);
    }

    #[test]
    fn speed_delay_hasted() {
        let mut effects = StatusEffects::default();
        effects.add(StatusEffectKind::Hasted, 5);
        assert_eq!(effects.speed_delay_multiplier(), 0.5);
    }

    #[test]
    fn speed_delay_slowed() {
        let mut effects = StatusEffects::default();
        effects.add(StatusEffectKind::Slowed, 5);
        assert_eq!(effects.speed_delay_multiplier(), 1.5);
    }

    #[test]
    fn speed_delay_hasted_and_slowed_cancel() {
        let mut effects = StatusEffects::default();
        effects.add(StatusEffectKind::Hasted, 5);
        effects.add(StatusEffectKind::Slowed, 5);
        // 1.0 * 0.5 * 1.5 = 0.75
        assert_eq!(effects.speed_delay_multiplier(), 0.75);
    }
}
