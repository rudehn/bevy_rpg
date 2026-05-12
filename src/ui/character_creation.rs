//! Character creation screen (Phase 2): two-section keyboard-driven
//! panel. Race on top, Class below. No attribute allocation step — the
//! attribute sum is fully derived from race + class.
//!
//! Flow:
//!   Main Menu → "New Game" → `AppState::CharacterCreation` → "Begin Descent"
//!   → `AppState::InGame` (with `CharacterChoice` resource overwritten).
//!
//! Keyboard model:
//!   - ↑/↓ cycle focus: Race / Class / Begin
//!   - ←/→ change the focused race or class
//!   - Enter on Begin: confirm and transition
//!   - Esc: return to main menu

use bevy::prelude::*;

use crate::character::{
    compose_attributes, derive_stats, Attribute, Attributes, CharacterChoice, Class, ClassAsset,
    ClassManifest, ClassManifestHandle, Race, RaceAsset, RaceManifest, RaceManifestHandle,
};
use crate::game::AppState;

const BG: Color = Color::srgb(0.04, 0.04, 0.04);
const PANEL_BG: Color = Color::srgb(0.10, 0.10, 0.10);
const PANEL_BG_FOCUSED: Color = Color::srgb(0.20, 0.20, 0.20);
const PANEL_BG_SELECTED: Color = Color::srgb(0.30, 0.25, 0.12);
const PANEL_BG_FOCUSED_SELECTED: Color = Color::srgb(0.45, 0.36, 0.16);
const GOLD: Color = Color::srgb(1.0, 0.84, 0.0);
const DIM: Color = Color::srgb(0.55, 0.55, 0.55);
const TEXT: Color = Color::srgb(0.85, 0.85, 0.85);

const RACES: [Race; 3] = [Race::Human, Race::Dwarf, Race::Elf];
const CLASSES: [Class; 4] = [Class::Warrior, Class::Rogue, Class::Mage, Class::Ranger];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Race,
    Class,
    Begin,
}

const TAB_ORDER: [Focus; 3] = [Focus::Race, Focus::Class, Focus::Begin];

#[derive(Resource, Debug, Clone)]
struct CharCreationDraft {
    race_idx: usize,
    class_idx: usize,
    focus: Focus,
}

impl Default for CharCreationDraft {
    fn default() -> Self {
        Self {
            race_idx: 0,
            class_idx: 0,
            focus: Focus::Race,
        }
    }
}

impl CharCreationDraft {
    fn race(&self) -> Race {
        RACES[self.race_idx]
    }
    fn class(&self) -> Class {
        CLASSES[self.class_idx]
    }
    fn cycle_focus(&mut self, backward: bool) {
        let cur = TAB_ORDER
            .iter()
            .position(|f| *f == self.focus)
            .unwrap_or(0);
        let next = if backward {
            (cur + TAB_ORDER.len() - 1) % TAB_ORDER.len()
        } else {
            (cur + 1) % TAB_ORDER.len()
        };
        self.focus = TAB_ORDER[next];
    }
}

#[derive(Component)]
struct OnCharCreationScreen;

#[derive(Component, Debug, Clone, Copy)]
struct RaceBoxMarker(Race);

#[derive(Component, Debug, Clone, Copy)]
struct ClassBoxMarker(Class);

#[derive(Component)]
struct BeginButtonMarker;

/// All live-updated text fields. Collapsed into one enum so a single
/// query handles all updates and we stay under Bevy's 16-param cap.
#[derive(Component, Debug, Clone, Copy)]
enum CharCreationText {
    AttrScore(Attribute),
    AttrMod(Attribute),
    Hp,
    Dodge,
    HitMelee,
    DamageMelee,
    HitRanged,
    DamageRanged,
    SpellDamage,
    RaceDesc,
    ClassDesc,
}

pub struct CharacterCreationPlugin;

impl Plugin for CharacterCreationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CharCreationDraft>()
            .add_systems(OnEnter(AppState::CharacterCreation), spawn_screen)
            .add_systems(
                Update,
                (handle_input, refresh_panels, refresh_text)
                    .chain()
                    .run_if(in_state(AppState::CharacterCreation)),
            )
            .add_systems(
                OnExit(AppState::CharacterCreation),
                despawn_screen::<OnCharCreationScreen>,
            );
    }
}

fn despawn_screen<T: Component>(query: Query<Entity, With<T>>, mut commands: Commands) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}

fn spawn_screen(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut draft: ResMut<CharCreationDraft>,
) {
    *draft = CharCreationDraft::default();
    let font = asset_server.load("fonts/Macondo-Regular.ttf");

    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::FlexStart,
                padding: UiRect::all(Val::Px(16.0)),
                row_gap: Val::Px(6.0),
                ..default()
            },
            BackgroundColor(BG),
            OnCharCreationScreen,
        ))
        .with_children(|root| {
            // Title
            root.spawn((
                Text::new("CHARACTER CREATION"),
                TextFont { font: font.clone(), font_size: 32.0, ..default() },
                TextColor(GOLD),
            ));

            // RACE row label
            root.spawn((
                Text::new("Race"),
                TextFont { font: font.clone(), font_size: 18.0, ..default() },
                TextColor(DIM),
                Node {
                    margin: UiRect::top(Val::Px(4.0)),
                    align_self: AlignSelf::FlexStart,
                    ..default()
                },
            ));

            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(12.0),
                ..default()
            })
            .with_children(|row| {
                for race in RACES {
                    spawn_choice_box(row, &font, race.name(), RaceBoxMarker(race));
                }
            });

            root.spawn((
                Text::new(""),
                TextFont { font: font.clone(), font_size: 14.0, ..default() },
                TextColor(TEXT),
                CharCreationText::RaceDesc,
            ));

            // CLASS row
            root.spawn((
                Text::new("Class"),
                TextFont { font: font.clone(), font_size: 18.0, ..default() },
                TextColor(DIM),
                Node {
                    margin: UiRect::top(Val::Px(4.0)),
                    align_self: AlignSelf::FlexStart,
                    ..default()
                },
            ));

            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(12.0),
                ..default()
            })
            .with_children(|row| {
                for class in CLASSES {
                    spawn_choice_box(row, &font, class.name(), ClassBoxMarker(class));
                }
            });

            root.spawn((
                Text::new(""),
                TextFont { font: font.clone(), font_size: 14.0, ..default() },
                TextColor(TEXT),
                CharCreationText::ClassDesc,
            ));

            // Attributes (read-only — no allocation in Phase 2)
            root.spawn((
                Text::new("Attributes (race + class)"),
                TextFont { font: font.clone(), font_size: 18.0, ..default() },
                TextColor(DIM),
                Node {
                    margin: UiRect::top(Val::Px(4.0)),
                    align_self: AlignSelf::FlexStart,
                    ..default()
                },
            ));

            for attr in [Attribute::Str, Attribute::Dex, Attribute::Int] {
                root.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(12.0),
                    padding: UiRect::all(Val::Px(3.0)),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        Text::new(attr.name().to_string()),
                        TextFont { font: font.clone(), font_size: 18.0, ..default() },
                        TextColor(TEXT),
                        Node { width: Val::Px(50.0), ..default() },
                    ));
                    row.spawn((
                        Text::new("0"),
                        TextFont { font: font.clone(), font_size: 18.0, ..default() },
                        TextColor(GOLD),
                        CharCreationText::AttrScore(attr),
                        Node { width: Val::Px(50.0), ..default() },
                    ));
                    row.spawn((
                        Text::new("(+0)"),
                        TextFont { font: font.clone(), font_size: 16.0, ..default() },
                        TextColor(DIM),
                        CharCreationText::AttrMod(attr),
                    ));
                });
            }

            // Preview section (live, race × class)
            root.spawn((
                Text::new("Preview (level 1)"),
                TextFont { font: font.clone(), font_size: 18.0, ..default() },
                TextColor(DIM),
                Node {
                    margin: UiRect::top(Val::Px(6.0)),
                    align_self: AlignSelf::FlexStart,
                    ..default()
                },
            ));
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(24.0),
                align_self: AlignSelf::FlexStart,
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Text::new("HP 13"),
                    TextFont { font: font.clone(), font_size: 16.0, ..default() },
                    TextColor(TEXT),
                    CharCreationText::Hp,
                ));
                row.spawn((
                    Text::new("Dodge 0"),
                    TextFont { font: font.clone(), font_size: 16.0, ..default() },
                    TextColor(TEXT),
                    CharCreationText::Dodge,
                ));
            });
            preview_row(root, &font, "Melee", CharCreationText::HitMelee, CharCreationText::DamageMelee);
            preview_row(root, &font, "Ranged", CharCreationText::HitRanged, CharCreationText::DamageRanged);
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(16.0),
                align_self: AlignSelf::FlexStart,
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Text::new("Spell"),
                    TextFont { font: font.clone(), font_size: 16.0, ..default() },
                    TextColor(DIM),
                    Node { width: Val::Px(72.0), ..default() },
                ));
                row.spawn((
                    Text::new("Damage +0"),
                    TextFont { font: font.clone(), font_size: 16.0, ..default() },
                    TextColor(TEXT),
                    CharCreationText::SpellDamage,
                ));
            });

            // Begin Descent
            root.spawn((
                Node {
                    margin: UiRect::top(Val::Px(8.0)),
                    padding: UiRect::all(Val::Px(6.0)),
                    ..default()
                },
                BackgroundColor(PANEL_BG),
                BeginButtonMarker,
            ))
            .with_children(|btn| {
                btn.spawn((
                    Text::new("Begin Descent"),
                    TextFont { font: font.clone(), font_size: 20.0, ..default() },
                    TextColor(GOLD),
                ));
            });

            // Help footer
            root.spawn((
                Text::new(
                    "\u{2191}/\u{2193}: next field   |   \u{2190}/\u{2192}: change race/class \
                    |   Enter (on Begin): start   |   Esc: cancel",
                ),
                TextFont { font: font.clone(), font_size: 12.0, ..default() },
                TextColor(DIM),
                Node {
                    margin: UiRect::top(Val::Px(8.0)),
                    ..default()
                },
            ));
        });
}

fn preview_row(
    parent: &mut ChildSpawnerCommands,
    font: &Handle<Font>,
    label: &str,
    hit_marker: CharCreationText,
    dmg_marker: CharCreationText,
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(16.0),
            align_self: AlignSelf::FlexStart,
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Text::new(label.to_string()),
                TextFont { font: font.clone(), font_size: 16.0, ..default() },
                TextColor(DIM),
                Node { width: Val::Px(72.0), ..default() },
            ));
            row.spawn((
                Text::new("Hit +0"),
                TextFont { font: font.clone(), font_size: 16.0, ..default() },
                TextColor(TEXT),
                hit_marker,
            ));
            row.spawn((
                Text::new("Damage +0"),
                TextFont { font: font.clone(), font_size: 16.0, ..default() },
                TextColor(TEXT),
                dmg_marker,
            ));
        });
}

fn spawn_choice_box<M: Component>(parent: &mut ChildSpawnerCommands, font: &Handle<Font>, label: &str, marker: M) {
    parent
        .spawn((
            Node {
                width: Val::Px(140.0),
                height: Val::Px(48.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(PANEL_BG),
            marker,
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(label.to_string()),
                TextFont { font: font.clone(), font_size: 20.0, ..default() },
                TextColor(TEXT),
            ));
        });
}

fn handle_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut draft: ResMut<CharCreationDraft>,
    mut next_state: ResMut<NextState<AppState>>,
    mut character_choice: ResMut<CharacterChoice>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        next_state.set(AppState::Menu);
        return;
    }

    if keys.just_pressed(KeyCode::ArrowDown) {
        draft.cycle_focus(false);
        return;
    }
    if keys.just_pressed(KeyCode::ArrowUp) {
        draft.cycle_focus(true);
        return;
    }

    let left = keys.just_pressed(KeyCode::ArrowLeft);
    let right = keys.just_pressed(KeyCode::ArrowRight);

    match draft.focus {
        Focus::Race => {
            if left && draft.race_idx > 0 {
                draft.race_idx -= 1;
            } else if right && draft.race_idx + 1 < RACES.len() {
                draft.race_idx += 1;
            }
        }
        Focus::Class => {
            if left && draft.class_idx > 0 {
                draft.class_idx -= 1;
            } else if right && draft.class_idx + 1 < CLASSES.len() {
                draft.class_idx += 1;
            }
        }
        Focus::Begin => {
            if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter) {
                character_choice.race = draft.race();
                character_choice.class = draft.class();
                next_state.set(AppState::InGame);
            }
        }
    }
}

fn panel_color(selected: bool, focused: bool) -> Color {
    match (selected, focused) {
        (true, true) => PANEL_BG_FOCUSED_SELECTED,
        (true, false) => PANEL_BG_SELECTED,
        (false, true) => PANEL_BG_FOCUSED,
        (false, false) => PANEL_BG,
    }
}

fn refresh_panels(
    draft: Res<CharCreationDraft>,
    mut race_boxes: Query<(&RaceBoxMarker, &mut BackgroundColor), Without<ClassBoxMarker>>,
    mut class_boxes: Query<
        (&ClassBoxMarker, &mut BackgroundColor),
        (Without<RaceBoxMarker>, Without<BeginButtonMarker>),
    >,
    mut begin_btn: Query<
        &mut BackgroundColor,
        (
            With<BeginButtonMarker>,
            Without<RaceBoxMarker>,
            Without<ClassBoxMarker>,
        ),
    >,
) {
    for (marker, mut bg) in &mut race_boxes {
        *bg = BackgroundColor(panel_color(
            marker.0 == draft.race(),
            draft.focus == Focus::Race,
        ));
    }
    for (marker, mut bg) in &mut class_boxes {
        *bg = BackgroundColor(panel_color(
            marker.0 == draft.class(),
            draft.focus == Focus::Class,
        ));
    }
    if let Ok(mut bg) = begin_btn.single_mut() {
        *bg = BackgroundColor(if draft.focus == Focus::Begin {
            PANEL_BG_FOCUSED_SELECTED
        } else {
            PANEL_BG
        });
    }
}

fn refresh_text(
    draft: Res<CharCreationDraft>,
    race_manifest_handle: Res<RaceManifestHandle>,
    race_manifests: Res<Assets<RaceManifest>>,
    class_manifest_handle: Res<ClassManifestHandle>,
    class_manifests: Res<Assets<ClassManifest>>,
    mut text_q: Query<(&CharCreationText, &mut Text)>,
) {
    let Some(race_manifest) = race_manifests.get(&race_manifest_handle.0) else { return };
    let Some(class_manifest) = class_manifests.get(&class_manifest_handle.0) else { return };

    let race_asset: Option<&RaceAsset> =
        race_manifest.races.get(&draft.race().name().to_lowercase());
    let class_asset: Option<&ClassAsset> = class_manifest
        .classes
        .get(&draft.class().name().to_lowercase());

    let attrs = if let (Some(r), Some(c)) = (race_asset, class_asset) {
        compose_attributes(r, c)
    } else {
        Attributes::default()
    };
    let derived = if let Some(r) = race_asset {
        derive_stats(r, &attrs, 1)
    } else {
        Default::default()
    };

    for (kind, mut t) in &mut text_q {
        let new = match *kind {
            CharCreationText::AttrScore(a) => attrs.get(a).to_string(),
            CharCreationText::AttrMod(a) => format!("({:+})", attrs.mod_of(a)),
            CharCreationText::Hp => format!("HP {}", derived.max_hp),
            CharCreationText::Dodge => format!("Dodge {}", derived.dodge),
            CharCreationText::HitMelee => format!("Hit {:+}", derived.hit_bonus_melee),
            CharCreationText::DamageMelee => format!("Damage {:+}", derived.damage_bonus_melee),
            CharCreationText::HitRanged => format!("Hit {:+}", derived.hit_bonus_ranged),
            CharCreationText::DamageRanged => format!("Damage {:+}", derived.damage_bonus_ranged),
            CharCreationText::SpellDamage => {
                format!("Damage {:+}", derived.damage_bonus_staff.max(0))
            }
            CharCreationText::RaceDesc => race_asset
                .map(|r| r.description.clone())
                .unwrap_or_default(),
            CharCreationText::ClassDesc => class_asset
                .map(|c| c.description.clone())
                .unwrap_or_default(),
        };
        *t = Text::new(new);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_focus_walks_full_order_forward() {
        let mut d = CharCreationDraft::default();
        let starts = vec![Focus::Race, Focus::Class, Focus::Begin, Focus::Race];
        let mut seen = vec![d.focus];
        for _ in 0..3 {
            d.cycle_focus(false);
            seen.push(d.focus);
        }
        assert_eq!(seen, starts);
    }

    #[test]
    fn cycle_focus_walks_backward() {
        let mut d = CharCreationDraft::default();
        d.cycle_focus(true); // Race → Begin (wrap)
        assert_eq!(d.focus, Focus::Begin);
        d.cycle_focus(true); // Begin → Class
        assert_eq!(d.focus, Focus::Class);
    }
}
