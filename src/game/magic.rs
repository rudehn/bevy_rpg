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

    /// Human-readable description with damage/effect details.
    pub fn description(&self, turns_remaining: u32) -> String {
        match self {
            Self::Burning { damage_per_turn } => format!("{} fire dmg/turn, {} turns", damage_per_turn, turns_remaining),
            Self::Poisoned { damage_per_turn } => format!("{} poison dmg/turn, {} turns", damage_per_turn, turns_remaining),
            Self::Hasted => format!("Move faster, {} turns", turns_remaining),
            Self::Slowed => format!("Move slower, {} turns", turns_remaining),
            Self::Stunned => format!("Cannot act, {} turns", turns_remaining),
            Self::Entangled => format!("Cannot move, {} turns", turns_remaining),
            Self::Enraged => format!("+50% damage, {} turns", turns_remaining),
            Self::FireResistance => format!("Immune to fire, {} turns", turns_remaining),
            Self::PoisonResistance => format!("Immune to poison, {} turns", turns_remaining),
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
    /// The original duration when the effect was first applied. Used for progress bar UI.
    #[serde(default = "default_initial_duration")]
    pub initial_duration: u32,
}

fn default_initial_duration() -> u32 { 1 }

/// Unified container for all status effects on an entity.
#[derive(Component, Debug, Clone, Default, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct StatusEffects(pub Vec<ActiveStatusEffect>);

impl StatusEffects {
    /// Add or refresh a status effect. If the same kind already exists, takes the longer duration.
    pub fn add(&mut self, kind: StatusEffectKind, turns: u32) {
        if let Some(existing) = self.0.iter_mut().find(|e| e.kind.same_kind(&kind)) {
            if turns > existing.turns_remaining {
                existing.turns_remaining = turns;
                existing.initial_duration = turns;
            }
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
            self.0.push(ActiveStatusEffect { kind, turns_remaining: turns, initial_duration: turns });
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

    pub fn is_burning(&self) -> bool {
        self.0.iter().any(|e| matches!(e.kind, StatusEffectKind::Burning { .. }))
    }

    pub fn is_poison_resistant(&self) -> bool {
        self.0.iter().any(|e| matches!(e.kind, StatusEffectKind::PoisonResistance))
    }

    pub fn is_fire_resistant(&self) -> bool {
        self.0.iter().any(|e| matches!(e.kind, StatusEffectKind::FireResistance))
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

    /// Returns display entries with duration info: (name, color, turns_remaining, initial_duration, description).
    pub fn display_entries_with_duration(&self) -> Vec<(&str, Color, u32, u32, String)> {
        self.0.iter().map(|e| {
            (e.kind.name(), e.kind.color(), e.turns_remaining, e.initial_duration, e.kind.description(e.turns_remaining))
        }).collect()
    }
}

// =====================================================================
// Tick Systems (run on TurnEndEvent)
// =====================================================================

/// Apply burning and poison damage-over-time each turn.
pub fn apply_dot_damage_system(
    mut turn_end: MessageReader<TurnEndEvent>,
    query: Query<(Entity, &StatusEffects, &Name)>,
    mut damage_writer: MessageWriter<ApplyDamageMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    if turn_end.read().count() == 0 { return; }

    for (entity, effects, name) in query.iter() {
        if let Some(dmg) = effects.burning_damage() {
            log_writer.write(GameLogMessage(format!(
                "{} takes {} fire damage from burning!", name.0, dmg
            )));
            damage_writer.write(ApplyDamageMessage {
                attacker: entity, target: entity,
                final_damage: dmg, damage_type: DamageType::Fire,
                source: DamageSource::Environment,
            });
        }
        if let Some(dmg) = effects.poison_damage() {
            log_writer.write(GameLogMessage(format!(
                "{} takes {} poison damage!", name.0, dmg
            )));
            damage_writer.write(ApplyDamageMessage {
                attacker: entity, target: entity,
                final_damage: dmg, damage_type: DamageType::Poison,
                source: DamageSource::Environment,
            });
        }
    }
}

/// Decrement all status effect durations and handle expirations.
pub fn tick_status_durations_system(
    mut turn_end: MessageReader<TurnEndEvent>,
    mut query: Query<(&mut StatusEffects, &Name, &crate::components::Position)>,
    mut decoration_writer: MessageWriter<crate::map::tile::DecorationMutationMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    if turn_end.read().count() == 0 { return; }

    for (mut effects, name, pos) in query.iter_mut() {
        let expired = effects.tick_all();
        for kind in expired {
            match kind {
                StatusEffectKind::Stunned => {
                    log_writer.write(GameLogMessage(format!("{} is no longer stunned.", name.0)));
                }
                StatusEffectKind::Entangled => {
                    log_writer.write(GameLogMessage(format!("{} breaks free of the cobwebs!", name.0)));
                    decoration_writer.write(crate::map::tile::DecorationMutationMessage {
                        position: bracket_lib::prelude::Point::new(pos.x, pos.y),
                        new_decoration: crate::map::tile::Decoration::None,
                    });
                }
                StatusEffectKind::Burning { .. } => {
                    log_writer.write(GameLogMessage(format!("{} is no longer burning.", name.0)));
                }
                StatusEffectKind::Poisoned { .. } => {
                    log_writer.write(GameLogMessage(format!("{} is no longer poisoned.", name.0)));
                }
                StatusEffectKind::FireResistance => {
                    log_writer.write(GameLogMessage(format!("{}'s fire resistance fades.", name.0)));
                }
                StatusEffectKind::PoisonResistance => {
                    log_writer.write(GameLogMessage(format!("{}'s poison resistance fades.", name.0)));
                }
                _ => {}
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

/// Count alive entities summoned by a specific summoner.
pub fn count_active_summons(summoner: Entity, world: &mut World) -> u32 {
    let mut query = world.query::<&crate::components::SummonedBy>();
    query.iter(world)
        .filter(|sb| sb.summoner == summoner)
        .count() as u32
}

/// Pick a monster name from a weighted list.
pub fn pick_weighted_monster(
    weights: &[(String, u32)],
    rng: &mut bracket_lib::random::RandomNumberGenerator,
) -> String {
    let total: u32 = weights.iter().map(|(_, w)| *w).sum();
    if total == 0 {
        return weights[0].0.clone();
    }
    let roll = rng.range(0, total as i32) as u32;
    let mut acc = 0u32;
    for (name, weight) in weights {
        acc += weight;
        if roll < acc {
            return name.clone();
        }
    }
    weights.last().unwrap().0.clone()
}

/// Resource written by ability handlers, consumed by process_pending_summon.
#[derive(Resource)]
pub struct PendingSummon {
    pub caster_pos: Position,
    pub caster_label: String,
    pub monster_name: String,
    pub count: u32,
    /// If set, summoned creatures get a SummonedBy component.
    pub caster_entity: Option<Entity>,
    /// If set, summoned creatures join this squad.
    pub squad_id: Option<crate::game::squad::SquadId>,
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
            let spawned_entity = crate::game::spawner::spawn_monster_by_name(
                &mut commands,
                &summon.monster_name,
                &point,
                &mut turn_manager,
                &monster_manifests,
                &monster_manifest_handle,
                &monster_sprite_assets,
                None,
            );
            if let Some(spawned_ent) = spawned_entity {
                if let Some(caster) = summon.caster_entity {
                    commands.entity(spawned_ent).insert(crate::components::SummonedBy { summoner: caster });
                }
                if let Some(sid) = summon.squad_id {
                    commands.entity(spawned_ent).insert((
                        sid,
                        crate::game::squad::SquadConfig {
                            on_leader_death: crate::game::squad::LeaderDeathBehavior::Scatter,
                            flee_threshold: 0.5,
                        },
                        crate::game::squad::Morale::new(0.6),
                    ));
                }
            }
        }
        log_writer.write(GameLogMessage(format!(
            "{} summons {} {}!",
            summon.caster_label, spawned, summon.monster_name
        )));
    }

    commands.remove_resource::<PendingSummon>();
}

// =====================================================================
// Post-spawn wiring
// =====================================================================

/// After floor materialization, attach `SummonedBy` to escort members of summoner squads.
/// This ensures escort rats (or other minions) spawned from the spawn table count toward
/// the summoner's cap and prevent the Broodmother from over-summoning on the first turn.
pub fn wire_summoner_escorts(
    leader_query: Query<
        (Entity, &crate::game::squad::SquadId, &crate::game::staves::MonsterAbilities),
        With<crate::game::squad::SquadLeader>,
    >,
    member_query: Query<
        (Entity, &crate::game::squad::SquadId),
        (With<crate::components::Monster>, Without<crate::game::squad::SquadLeader>),
    >,
    existing_summons: Query<&crate::components::SummonedBy>,
    mut commands: Commands,
) {
    for (leader_entity, leader_squad, abilities) in leader_query.iter() {
        let has_summon_cap = abilities.0.iter().any(|a| {
            matches!(
                a.kind,
                crate::game::staves::MonsterAbilityKind::SummonCapped { .. }
            )
        });
        if !has_summon_cap {
            continue;
        }

        for (member_entity, member_squad) in member_query.iter() {
            if member_squad.0 != leader_squad.0 {
                continue;
            }
            // Don't double-attach if already has SummonedBy
            if existing_summons.get(member_entity).is_ok() {
                continue;
            }
            commands
                .entity(member_entity)
                .insert(crate::components::SummonedBy {
                    summoner: leader_entity,
                });
        }
    }
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
                    // tick_status_effects_system is registered in ProcessingPhase::Cleanup
                    // (turns.rs) so its mutations get processed in the same chain.
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

    #[test]
    fn count_active_summons_zero_when_none_exist() {
        let mut world = World::new();
        let summoner = world.spawn_empty().id();
        let count = count_active_summons(summoner, &mut world);
        assert_eq!(count, 0);
    }

    #[test]
    fn count_active_summons_counts_matching_entities() {
        let mut world = World::new();
        let summoner = world.spawn_empty().id();
        let other = world.spawn_empty().id();
        world.spawn(crate::components::SummonedBy { summoner });
        world.spawn(crate::components::SummonedBy { summoner });
        world.spawn(crate::components::SummonedBy { summoner: other });
        let count = count_active_summons(summoner, &mut world);
        assert_eq!(count, 2);
    }

    #[test]
    fn count_active_summons_excludes_despawned() {
        let mut world = World::new();
        let summoner = world.spawn_empty().id();
        let minion = world.spawn(crate::components::SummonedBy { summoner }).id();
        world.spawn(crate::components::SummonedBy { summoner });
        assert_eq!(count_active_summons(summoner, &mut world), 2);
        world.despawn(minion);
        assert_eq!(count_active_summons(summoner, &mut world), 1);
    }

    #[test]
    fn pick_weighted_monster_always_picks_only_nonzero() {
        let weights = vec![("Sewer Rat".to_string(), 100u32), ("Plague Rat".to_string(), 0u32)];
        let mut rng = bracket_lib::random::RandomNumberGenerator::new();
        for _ in 0..20 {
            assert_eq!(pick_weighted_monster(&weights, &mut rng), "Sewer Rat");
        }
    }

    #[test]
    fn pick_weighted_monster_single_entry() {
        let weights = vec![("Plague Rat".to_string(), 30u32)];
        let mut rng = bracket_lib::random::RandomNumberGenerator::new();
        assert_eq!(pick_weighted_monster(&weights, &mut rng), "Plague Rat");
    }
}
