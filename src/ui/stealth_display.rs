//! Shared "Notice this turn: X%" Stealth UI block.
//!
//! Used by both the monster info hover tooltip (`monster_info.rs`)
//! and any future monster inspection overlay so the breakdown stays
//! consistent. Computes the player → monster perception delta from
//! live components and emits Bevy text children describing the result.

use bevy::ecs::relationship::RelatedSpawnerCommands;
use bevy::prelude::*;
use bracket_lib::prelude::Point;

use crate::character::Attributes;
use crate::game::items::{Equipment, ItemProperties};
use crate::game::skills::Skills;
use crate::game::stealth::{
    compute_perception_components, compute_stealth_components, equipped_armor_stealth_penalty,
    PerceptionComponents, StealthComponents,
};
use crate::map::Map;
use crate::map::light::LightMap;
use roguelike_engine::stealth::{notice_probability, Awareness, AwarenessState, NoiseMap};

/// Resolved text + breakdown for the monster Stealth section.
///
/// `headline` is always present (one of the early-out strings, or the
/// formatted "Notice this turn: NN%"). `perception` and `stealth` are
/// `Some` only when an actual roll could happen — i.e. when the
/// monster has line of sight on the player and isn't already Aware.
pub struct StealthDisplayLines {
    pub headline: String,
    pub perception: Option<PerceptionComponents>,
    pub stealth: Option<StealthComponents>,
}

/// Read the tile light intensity at `pos` out of [`LightMap`].
///
/// Duplicates the helper in `src/game/stealth.rs` so this module stays
/// free of cross-crate visibility constraints. `LightMap` carries no
/// width/height of its own — the buffer is sized by the active [`Map`].
pub fn light_intensity_at(light_map: &LightMap, map: &Map, pos: Point) -> f32 {
    if pos.x < 0 || pos.y < 0 || pos.x >= map.width || pos.y >= map.height {
        return 0.0;
    }
    let idx = map.xy_idx(pos.x, pos.y);
    light_map.values.get(idx).copied().unwrap_or(0.0)
}

/// Build the headline + breakdown for the monster Stealth section.
///
/// Early-out cases:
/// * Monster is already [`AwarenessState::Aware`] → "Already aware"
/// * Monster has no line of sight on the player → "Out of sight"
///
/// Otherwise compute the opposed-roll delta and translate to a
/// percentage via [`notice_probability`].
#[allow(clippy::too_many_arguments)]
pub fn stealth_display_for(
    monster_perception: i32,
    monster_pos: Point,
    is_asleep: bool,
    in_viewshed: bool,
    monster_awareness: Option<&Awareness>,
    player_entity: Entity,
    player_pos: Point,
    player_skills: Option<&Skills>,
    player_attrs: Option<&Attributes>,
    player_armor_penalty: i32,
    light_intensity_at_player: f32,
    noise_map: &NoiseMap,
) -> StealthDisplayLines {
    let state = monster_awareness.and_then(|a| a.get(player_entity)).map(|r| r.state);
    if matches!(state, Some(AwarenessState::Aware)) {
        return StealthDisplayLines {
            headline: "Already aware".to_string(),
            perception: None,
            stealth: None,
        };
    }
    if !in_viewshed {
        return StealthDisplayLines {
            headline: "Out of sight".to_string(),
            perception: None,
            stealth: None,
        };
    }
    let dist = (player_pos.x - monster_pos.x)
        .abs()
        .max((player_pos.y - monster_pos.y).abs());
    let perc = compute_perception_components(monster_perception, is_asleep, dist);
    let stealth = compute_stealth_components(
        player_skills,
        player_attrs,
        player_armor_penalty,
        player_pos,
        light_intensity_at_player,
        noise_map,
    );
    let delta = perc.total() - stealth.total();
    let pct = (notice_probability(delta) * 100.0).round() as i32;
    StealthDisplayLines {
        headline: format!("Notice this turn: {}%", pct),
        perception: Some(perc),
        stealth: Some(stealth),
    }
}

/// Render the Stealth section as Bevy UI children of `parent`.
///
/// Layout: a divider, a headline line, and (when a roll happened) a
/// multi-line indented breakdown of every modifier contributing to
/// the perception and stealth totals. Caller is responsible for
/// providing the font handle so styling stays consistent with the
/// surrounding tooltip / panel.
pub fn render_stealth_section(
    parent: &mut RelatedSpawnerCommands<ChildOf>,
    font: Handle<Font>,
    lines: &StealthDisplayLines,
) {
    parent.spawn((
        Text::new("Stealth:"),
        TextFont {
            font: font.clone(),
            font_size: 13.0,
            ..Default::default()
        },
        TextColor(Color::srgb(0.55, 0.85, 0.65)),
        Node {
            margin: UiRect::top(Val::Px(4.0)),
            ..Default::default()
        },
    ));
    parent.spawn((
        Text::new(lines.headline.clone()),
        TextFont {
            font: font.clone(),
            font_size: 12.0,
            ..Default::default()
        },
        TextColor(Color::srgb(0.9, 0.9, 0.9)),
        Node {
            padding: UiRect::left(Val::Px(8.0)),
            ..Default::default()
        },
    ));
    if let (Some(p), Some(s)) = (lines.perception, lines.stealth) {
        let body = format!(
            "Perception: {:+} (base {:+}, close {:+}, asleep {:+})\nStealth:    {:+} (skill {:+}, DEX {:+}, armor {:+}, light {:+}, noise {:+})",
            p.total(),
            p.base,
            p.close_range_bonus,
            p.asleep_penalty,
            s.total(),
            s.skill_half,
            s.dex_mod,
            -s.armor_penalty,
            s.light_mod,
            s.noise_mod,
        );
        parent.spawn((
            Text::new(body),
            TextFont {
                font,
                font_size: 10.0,
                ..Default::default()
            },
            TextColor(Color::srgb(0.7, 0.7, 0.7)),
            Node {
                padding: UiRect::left(Val::Px(8.0)),
                ..Default::default()
            },
        ));
    }
}

/// Convenience wrapper: build the armor stealth penalty for the
/// player's equipped gear. Returns 0 when the player has nothing
/// equipped in the armor slots.
pub fn player_armor_stealth_penalty(
    equipment: Option<&Equipment>,
    item_query: &Query<&ItemProperties>,
) -> i32 {
    equipment
        .map(|eq| equipped_armor_stealth_penalty(eq, item_query))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pe() -> Entity {
        Entity::from_raw_u32(1).expect("valid test entity index")
    }

    #[test]
    fn already_aware_short_circuits() {
        let mut a = Awareness::default();
        a.set(pe(), AwarenessState::Aware, 0);
        let noise = NoiseMap::new(80, 60);
        let lines = stealth_display_for(
            0,
            Point::new(5, 5),
            false,
            true,
            Some(&a),
            pe(),
            Point::new(4, 5),
            None,
            None,
            0,
            0.5,
            &noise,
        );
        assert_eq!(lines.headline, "Already aware");
        assert!(lines.perception.is_none());
        assert!(lines.stealth.is_none());
    }

    #[test]
    fn out_of_sight_short_circuits() {
        let a = Awareness::default();
        let noise = NoiseMap::new(80, 60);
        let lines = stealth_display_for(
            0,
            Point::new(5, 5),
            false,
            false,
            Some(&a),
            pe(),
            Point::new(4, 5),
            None,
            None,
            0,
            0.5,
            &noise,
        );
        assert_eq!(lines.headline, "Out of sight");
        assert!(lines.perception.is_none());
        assert!(lines.stealth.is_none());
    }

    #[test]
    fn visible_unaware_emits_percentage() {
        let a = Awareness::default();
        let noise = NoiseMap::new(80, 60);
        let lines = stealth_display_for(
            0,
            Point::new(5, 5),
            false,
            true,
            Some(&a),
            pe(),
            Point::new(4, 5),
            None,
            None,
            0,
            0.5,
            &noise,
        );
        assert!(lines.headline.starts_with("Notice this turn:"));
        assert!(lines.perception.is_some());
        assert!(lines.stealth.is_some());
    }

    #[test]
    fn missing_awareness_component_treated_as_unaware() {
        let noise = NoiseMap::new(80, 60);
        let lines = stealth_display_for(
            0,
            Point::new(5, 5),
            false,
            true,
            None,
            pe(),
            Point::new(4, 5),
            None,
            None,
            0,
            0.5,
            &noise,
        );
        assert!(lines.headline.starts_with("Notice this turn:"));
    }

    #[test]
    fn percentage_clamped_between_0_and_100() {
        let a = Awareness::default();
        let noise = NoiseMap::new(80, 60);
        // Far-away monster with zero perception, dark tile (light_mod
        // = +3): stealth strongly favoured.
        let lines = stealth_display_for(
            -20,
            Point::new(50, 50),
            false,
            true,
            Some(&a),
            pe(),
            Point::new(4, 5),
            None,
            None,
            0,
            0.0,
            &noise,
        );
        // Extract the percentage number from the headline.
        let pct_str = lines
            .headline
            .trim_end_matches('%')
            .rsplit(' ')
            .next()
            .unwrap();
        let pct: i32 = pct_str.parse().unwrap();
        assert!(
            (0..=100).contains(&pct),
            "percentage {} out of range",
            pct
        );
    }
}
