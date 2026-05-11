use bevy::prelude::*;

use crate::{
    components::{Name, Position},
    game::AppState,
    map::Map,
    ui::game_log::GameLogMessage,
};

// =====================================================================
// Engine re-exports
// =====================================================================
//
// Status effect types now live in the engine. We re-export them so the
// rest of the game crate can keep using `crate::game::magic::{...}`.
pub use roguelike_engine::status::{
    compute_damage_modifier, compute_speed_modifier, status_effect_tick_system,
    StatusAppliedEvent, StatusEffectInstance, StatusEffectKind, StatusEffectPlugin,
    StatusEffectSet, StatusEffects, StatusExpiredEvent,
};

// =====================================================================
// Game-specific custom status IDs
// =====================================================================
//
// The engine ships a blessed set (Burning, Poisoned, Stunned, Hasted,
// Slowed, Strengthened, Weakened). Everything else the game needs is
// modelled as `StatusEffectKind::Custom { id }`.
//
// These IDs must remain stable for save-file compatibility.
pub const STATUS_ENTANGLED: u32 = 1;
pub const STATUS_ENRAGED: u32 = 2;
pub const STATUS_FIRE_RESISTANCE: u32 = 3;
pub const STATUS_POISON_RESISTANCE: u32 = 4;

// =====================================================================
// Game-side status metadata (display / UI)
// =====================================================================

/// Registry for user-facing metadata on `StatusEffectKind::Custom` values.
/// Games insert this as a resource and populate it at startup.
#[derive(Resource, Default, Clone)]
pub struct StatusEffectRegistry {
    entries: std::collections::HashMap<u32, CustomStatusEntry>,
}

#[derive(Clone)]
pub struct CustomStatusEntry {
    pub name: &'static str,
    pub color: Color,
    pub describe: fn(turns_remaining: u32) -> String,
}

impl StatusEffectRegistry {
    pub fn register(&mut self, id: u32, entry: CustomStatusEntry) {
        self.entries.insert(id, entry);
    }

    pub fn get(&self, id: u32) -> Option<&CustomStatusEntry> {
        self.entries.get(&id)
    }
}

/// Human-readable name for a status kind (includes game-specific custom kinds).
pub fn kind_name(kind: &StatusEffectKind) -> &'static str {
    match kind {
        StatusEffectKind::Burning => "Burning",
        StatusEffectKind::Poisoned => "Poisoned",
        StatusEffectKind::Stunned => "Stunned",
        StatusEffectKind::Hasted => "Hasted",
        StatusEffectKind::Slowed => "Slowed",
        StatusEffectKind::Strengthened => "Strengthened",
        StatusEffectKind::Weakened => "Weakened",
        StatusEffectKind::Custom { id: STATUS_ENTANGLED } => "Entangled",
        StatusEffectKind::Custom { id: STATUS_ENRAGED } => "Enraged",
        StatusEffectKind::Custom { id: STATUS_FIRE_RESISTANCE } => "Fire Resistance",
        StatusEffectKind::Custom { id: STATUS_POISON_RESISTANCE } => "Poison Resistance",
        _ => "Custom",
    }
}

/// UI color for a status kind.
pub fn kind_color(kind: &StatusEffectKind) -> Color {
    match kind {
        StatusEffectKind::Burning => Color::srgb(1.0, 0.5, 0.1),
        StatusEffectKind::Poisoned => Color::srgb(0.3, 0.9, 0.3),
        StatusEffectKind::Stunned => Color::srgb(1.0, 1.0, 0.0),
        StatusEffectKind::Hasted => Color::srgb(1.0, 1.0, 0.3),
        StatusEffectKind::Slowed => Color::srgb(0.5, 0.5, 0.9),
        StatusEffectKind::Strengthened => Color::srgb(1.0, 0.7, 0.7),
        StatusEffectKind::Weakened => Color::srgb(0.7, 0.7, 0.7),
        StatusEffectKind::Custom { id: STATUS_ENTANGLED } => Color::srgb(0.8, 0.8, 0.8),
        StatusEffectKind::Custom { id: STATUS_ENRAGED } => Color::srgb(0.9, 0.2, 0.2),
        StatusEffectKind::Custom { id: STATUS_FIRE_RESISTANCE } => Color::srgb(1.0, 0.6, 0.2),
        StatusEffectKind::Custom { id: STATUS_POISON_RESISTANCE } => Color::srgb(0.4, 1.0, 0.4),
        _ => Color::srgb(0.8, 0.8, 0.8),
    }
}

/// Human-readable description for a status kind, using its magnitude / duration.
pub fn kind_description(kind: &StatusEffectKind, turns_remaining: u32, magnitude: i32) -> String {
    match kind {
        StatusEffectKind::Burning => format!("{} fire dmg/turn, {} turns", magnitude, turns_remaining),
        StatusEffectKind::Poisoned => format!("{} poison dmg/turn, {} turns", magnitude, turns_remaining),
        StatusEffectKind::Hasted => format!("Move faster, {} turns", turns_remaining),
        StatusEffectKind::Slowed => format!("Move slower, {} turns", turns_remaining),
        StatusEffectKind::Stunned => format!("Cannot act, {} turns", turns_remaining),
        StatusEffectKind::Strengthened => format!("+50% damage, {} turns", turns_remaining),
        StatusEffectKind::Weakened => format!("-25% damage, {} turns", turns_remaining),
        StatusEffectKind::Custom { id: STATUS_ENTANGLED } => format!("Cannot move, {} turns", turns_remaining),
        StatusEffectKind::Custom { id: STATUS_ENRAGED } => format!("+50% damage, {} turns", turns_remaining),
        StatusEffectKind::Custom { id: STATUS_FIRE_RESISTANCE } => format!("Immune to fire, {} turns", turns_remaining),
        StatusEffectKind::Custom { id: STATUS_POISON_RESISTANCE } => format!("Immune to poison, {} turns", turns_remaining),
        _ => format!("{} turns", turns_remaining),
    }
}

/// Metadata lookup that respects the [`StatusEffectRegistry`] for unknown `Custom` kinds.
pub fn kind_metadata_with(
    kind: &StatusEffectKind,
    turns_remaining: u32,
    magnitude: i32,
    registry: Option<&StatusEffectRegistry>,
) -> (&'static str, Color, String) {
    if let (StatusEffectKind::Custom { id }, Some(reg)) = (kind, registry) {
        // Only consult registry for game-unknown ids; our blessed custom ids
        // still use the built-in metadata above.
        if !matches!(
            *id,
            STATUS_ENTANGLED | STATUS_ENRAGED | STATUS_FIRE_RESISTANCE | STATUS_POISON_RESISTANCE
        ) {
            if let Some(entry) = reg.get(*id) {
                return (entry.name, entry.color, (entry.describe)(turns_remaining));
            }
        }
    }
    (
        kind_name(kind),
        kind_color(kind),
        kind_description(kind, turns_remaining, magnitude),
    )
}

// =====================================================================
// GameStatusEffectsExt — convenience layer
// =====================================================================
//
// The engine's `StatusEffects` API is `add(StatusEffectInstance)` /
// `has(kind)` / `magnitude_of(kind)`. Game code historically called
// `effects.add(kind, turns)`, `effects.is_stunned()`, etc. This trait
// preserves that vocabulary so the hundreds of call sites don't churn.

pub trait GameStatusEffectsExt {
    fn add_effect(&mut self, kind: StatusEffectKind, turns: u32);
    fn add_effect_with_magnitude(
        &mut self,
        kind: StatusEffectKind,
        turns: u32,
        magnitude: i32,
        source: Option<Entity>,
    );
    fn remove_kind(&mut self, matcher: impl Fn(&StatusEffectKind) -> bool);

    fn is_stunned(&self) -> bool;
    fn is_entangled(&self) -> bool;
    fn is_hasted(&self) -> bool;
    fn is_slowed(&self) -> bool;
    fn is_enraged(&self) -> bool;
    fn is_poisoned(&self) -> bool;
    fn is_burning(&self) -> bool;
    fn is_poison_resistant(&self) -> bool;
    fn is_fire_resistant(&self) -> bool;

    fn burning_damage(&self) -> Option<i32>;
    fn poison_damage(&self) -> Option<i32>;
    fn speed_delay_multiplier(&self) -> f32;

    fn display_entries(&self) -> Vec<(&'static str, Color)>;
    fn display_entries_with_duration(&self) -> Vec<(&'static str, Color, u32, u32, String)>;
}

impl GameStatusEffectsExt for StatusEffects {
    fn add_effect(&mut self, kind: StatusEffectKind, turns: u32) {
        self.add(StatusEffectInstance {
            kind,
            remaining_turns: turns,
            magnitude: 0,
            source: None,
        });
    }

    fn add_effect_with_magnitude(
        &mut self,
        kind: StatusEffectKind,
        turns: u32,
        magnitude: i32,
        source: Option<Entity>,
    ) {
        self.add(StatusEffectInstance {
            kind,
            remaining_turns: turns,
            magnitude,
            source,
        });
    }

    fn remove_kind(&mut self, matcher: impl Fn(&StatusEffectKind) -> bool) {
        self.effects.retain(|e| !matcher(&e.kind));
    }

    fn is_stunned(&self) -> bool {
        self.has(StatusEffectKind::Stunned)
    }

    fn is_entangled(&self) -> bool {
        self.has(StatusEffectKind::Custom { id: STATUS_ENTANGLED })
    }

    fn is_hasted(&self) -> bool {
        self.has(StatusEffectKind::Hasted)
    }

    fn is_slowed(&self) -> bool {
        self.has(StatusEffectKind::Slowed)
    }

    fn is_enraged(&self) -> bool {
        self.has(StatusEffectKind::Custom { id: STATUS_ENRAGED })
    }

    fn is_poisoned(&self) -> bool {
        self.has(StatusEffectKind::Poisoned)
    }

    fn is_burning(&self) -> bool {
        self.has(StatusEffectKind::Burning)
    }

    fn is_poison_resistant(&self) -> bool {
        self.has(StatusEffectKind::Custom { id: STATUS_POISON_RESISTANCE })
    }

    fn is_fire_resistant(&self) -> bool {
        self.has(StatusEffectKind::Custom { id: STATUS_FIRE_RESISTANCE })
    }

    fn burning_damage(&self) -> Option<i32> {
        let m = self.magnitude_of(StatusEffectKind::Burning);
        if m > 0 { Some(m) } else { None }
    }

    fn poison_damage(&self) -> Option<i32> {
        let m = self.magnitude_of(StatusEffectKind::Poisoned);
        if m > 0 { Some(m) } else { None }
    }

    fn speed_delay_multiplier(&self) -> f32 {
        compute_speed_modifier(self).clamp(0.5, 2.0)
    }

    fn display_entries(&self) -> Vec<(&'static str, Color)> {
        self.effects
            .iter()
            .map(|e| (kind_name(&e.kind), kind_color(&e.kind)))
            .collect()
    }

    fn display_entries_with_duration(&self) -> Vec<(&'static str, Color, u32, u32, String)> {
        self.effects
            .iter()
            .map(|e| {
                let initial = e.remaining_turns.max(1);
                (
                    kind_name(&e.kind),
                    kind_color(&e.kind),
                    e.remaining_turns,
                    initial,
                    kind_description(&e.kind, e.remaining_turns, e.magnitude),
                )
            })
            .collect()
    }
}

// =====================================================================
// Game-side reaction systems
// =====================================================================

/// Logs status expiration messages and cleans up the cobweb decoration
/// when an Entangled effect expires.
///
/// Replaces the per-expiry logic that used to live in the game's
/// `tick_status_durations_system`. The engine's tick system now does the
/// actual bookkeeping and emits `StatusExpiredEvent`.
pub fn status_expiry_log_system(
    mut events: MessageReader<StatusExpiredEvent>,
    query: Query<(&Name, &Position)>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut decoration_writer: MessageWriter<crate::map::tile::DecorationMutationMessage>,
) {
    for event in events.read() {
        let Ok((name, pos)) = query.get(event.entity) else {
            continue;
        };
        match event.kind {
            StatusEffectKind::Stunned => {
                log_writer.write(GameLogMessage(format!("{} is no longer stunned.", name.0)));
            }
            StatusEffectKind::Burning => {
                log_writer.write(GameLogMessage(format!("{} is no longer burning.", name.0)));
            }
            StatusEffectKind::Poisoned => {
                log_writer.write(GameLogMessage(format!("{} is no longer poisoned.", name.0)));
            }
            StatusEffectKind::Custom { id: STATUS_ENTANGLED } => {
                log_writer.write(GameLogMessage(format!(
                    "{} breaks free of the cobwebs!",
                    name.0
                )));
                decoration_writer.write(crate::map::tile::DecorationMutationMessage {
                    position: bracket_lib::prelude::Point::new(pos.x, pos.y),
                    new_decoration: crate::map::tile::Decoration::None,
                });
            }
            StatusEffectKind::Custom { id: STATUS_FIRE_RESISTANCE } => {
                log_writer.write(GameLogMessage(format!(
                    "{}'s fire resistance fades.",
                    name.0
                )));
            }
            StatusEffectKind::Custom { id: STATUS_POISON_RESISTANCE } => {
                log_writer.write(GameLogMessage(format!(
                    "{}'s poison resistance fades.",
                    name.0
                )));
            }
            _ => {}
        }
    }
}

/// Apply speed multipliers from `StatusEffects`. Recomputes both movement
/// and attack delays each frame so the base innate speed is preserved
/// while temporary buffs/debuffs layer on top.
pub fn apply_speed_effects_system(
    mut query: Query<(&mut crate::game::actions::SpeedStats, &StatusEffects)>,
) {
    for (mut speed, effects) in query.iter_mut() {
        let multiplier = compute_speed_modifier(effects).clamp(0.5, 2.0);
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
    query
        .iter(world)
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
    let Some(summon) = pending else { return };

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
                    commands
                        .entity(spawned_ent)
                        .insert(crate::components::SummonedBy { summoner: caster });
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

/// After floor materialization, attach `SummonedBy` to escort members of
/// summoner squads.
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
        app.add_plugins(StatusEffectPlugin)
            .init_resource::<StatusEffectRegistry>()
            .add_systems(
                Update,
                (apply_speed_effects_system, process_pending_summon)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn burning(dmg: i32, turns: u32) -> StatusEffectInstance {
        StatusEffectInstance {
            kind: StatusEffectKind::Burning,
            remaining_turns: turns,
            magnitude: dmg,
            source: None,
        }
    }

    fn poisoned(dmg: i32, turns: u32) -> StatusEffectInstance {
        StatusEffectInstance {
            kind: StatusEffectKind::Poisoned,
            remaining_turns: turns,
            magnitude: dmg,
            source: None,
        }
    }

    #[test]
    fn add_burning_effect() {
        let mut effects = StatusEffects::default();
        effects.add(burning(3, 5));
        assert_eq!(effects.burning_damage(), Some(3));
    }

    #[test]
    fn remove_burning_via_remove_kind() {
        let mut effects = StatusEffects::default();
        effects.add(burning(3, 5));
        effects.add(poisoned(2, 3));
        assert!(effects.burning_damage().is_some());
        assert!(effects.poison_damage().is_some());

        effects.remove_kind(|k| matches!(k, StatusEffectKind::Burning));

        assert!(effects.burning_damage().is_none());
        assert!(effects.poison_damage().is_some());
    }

    #[test]
    fn remove_burning_when_none_is_noop() {
        let mut effects = StatusEffects::default();
        effects.add(poisoned(2, 3));
        effects.remove_kind(|k| matches!(k, StatusEffectKind::Burning));
        assert!(effects.poison_damage().is_some());
    }

    #[test]
    fn refresh_takes_longer_duration() {
        let mut effects = StatusEffects::default();
        effects.add(burning(3, 2));
        effects.add(burning(5, 10));
        // Engine's add() takes max(duration) and max(magnitude).
        assert_eq!(effects.burning_damage(), Some(5));
        assert_eq!(effects.effects.len(), 1);
    }

    #[test]
    fn speed_delay_hasted() {
        let mut effects = StatusEffects::default();
        effects.add_effect(StatusEffectKind::Hasted, 5);
        assert_eq!(effects.speed_delay_multiplier(), 0.5);
    }

    #[test]
    fn speed_delay_slowed() {
        let mut effects = StatusEffects::default();
        effects.add_effect(StatusEffectKind::Slowed, 5);
        assert_eq!(effects.speed_delay_multiplier(), 1.5);
    }

    #[test]
    fn speed_delay_hasted_and_slowed_stack() {
        let mut effects = StatusEffects::default();
        effects.add_effect(StatusEffectKind::Hasted, 5);
        effects.add_effect(StatusEffectKind::Slowed, 5);
        // compute_speed_modifier: 1.0 * 0.5 * 1.5 = 0.75
        assert_eq!(effects.speed_delay_multiplier(), 0.75);
    }

    #[test]
    fn entangled_via_custom_id() {
        let mut effects = StatusEffects::default();
        effects.add_effect(StatusEffectKind::Custom { id: STATUS_ENTANGLED }, 3);
        assert!(effects.is_entangled());
        assert!(!effects.is_stunned());
    }

    #[test]
    fn fire_resistance_via_custom_id() {
        let mut effects = StatusEffects::default();
        effects.add_effect(StatusEffectKind::Custom { id: STATUS_FIRE_RESISTANCE }, 3);
        assert!(effects.is_fire_resistant());
        assert!(!effects.is_poison_resistant());
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

    #[test]
    fn custom_status_distinct_ids_stack_separately() {
        let mut effects = StatusEffects::default();
        effects.add_effect(StatusEffectKind::Custom { id: 11 }, 3);
        effects.add_effect(StatusEffectKind::Custom { id: 12 }, 5);
        assert_eq!(effects.effects.len(), 2);
    }

    #[test]
    fn custom_status_same_id_refreshes() {
        let mut effects = StatusEffects::default();
        effects.add_effect(StatusEffectKind::Custom { id: 7 }, 3);
        effects.add_effect(StatusEffectKind::Custom { id: 7 }, 10);
        assert_eq!(effects.effects.len(), 1);
        assert_eq!(effects.effects[0].remaining_turns, 10);
    }

    #[test]
    fn custom_status_registry_provides_metadata() {
        fn describe_frozen(turns: u32) -> String {
            format!("Frozen solid, {} turns", turns)
        }
        let mut registry = StatusEffectRegistry::default();
        registry.register(
            100,
            CustomStatusEntry {
                name: "Frozen",
                color: Color::srgb(0.3, 0.5, 1.0),
                describe: describe_frozen,
            },
        );

        let kind = StatusEffectKind::Custom { id: 100 };
        let (name, _color, desc) = kind_metadata_with(&kind, 4, 0, Some(&registry));
        assert_eq!(name, "Frozen");
        assert_eq!(desc, "Frozen solid, 4 turns");
    }

    #[test]
    fn custom_status_metadata_falls_back_without_registry() {
        let kind = StatusEffectKind::Custom { id: 99 };
        let (name, _color, _desc) = kind_metadata_with(&kind, 3, 0, None);
        assert_eq!(name, "Custom");
    }
}
