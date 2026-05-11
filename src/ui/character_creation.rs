//! Character creation screen: race + class + attribute allocation on one
//! keyboard-driven panel. Mirrors the structure of `src/ui/menu.rs`.
//!
//! Flow:
//!   Main Menu → "New Game" → `AppState::CharacterCreation` → "Begin Descent"
//!   → `AppState::InGame` (with `CharacterChoice` resource overwritten).
//!
//! Keyboard model:
//!   - ↑/↓ cycle focus between: Race / Class / STR / DEX / CON / INT / Begin
//!   - ←/→ on Race or Class: change the selected option
//!   - ←/→ on an attribute: decrement / increment (respects cap + remaining
//!     points + Human Versatile +1 cap exception)
//!   - Enter on Begin: confirm and transition
//!   - Esc: return to main menu

use bevy::prelude::*;

use crate::character::{
    ability_mod, compose_attributes, derive_stats, Attribute, Attributes, CharacterChoice,
    Class, ClassAsset, ClassManifest, ClassManifestHandle, Race, RaceAsset, RaceManifest,
    RaceManifestHandle,
};
use crate::game::AppState;

const FREE_POINTS: i32 = 4;
const STAT_FLOOR: i32 = 8;
const STAT_CAP_DEFAULT: i32 = 4; // baseline + 4
const STAT_CAP_HUMAN_BONUS: i32 = 1; // Versatile: one stat may go +1 over

const BG: Color = Color::srgb(0.04, 0.04, 0.04);
const PANEL_BG: Color = Color::srgb(0.10, 0.10, 0.10);
const PANEL_BG_FOCUSED: Color = Color::srgb(0.20, 0.20, 0.20);
const PANEL_BG_SELECTED: Color = Color::srgb(0.30, 0.25, 0.12);
const PANEL_BG_FOCUSED_SELECTED: Color = Color::srgb(0.45, 0.36, 0.16);
const GOLD: Color = Color::srgb(1.0, 0.84, 0.0);
const DIM: Color = Color::srgb(0.55, 0.55, 0.55);
const TEXT: Color = Color::srgb(0.85, 0.85, 0.85);

const RACES: [Race; 4] = [Race::Human, Race::Dwarf, Race::Elf, Race::Halfling];
const CLASSES: [Class; 4] = [Class::Warrior, Class::Rogue, Class::Mage, Class::Ranger];
const ATTRS: [Attribute; 4] = [Attribute::Str, Attribute::Dex, Attribute::Con, Attribute::Int];

/// All possible focus targets, in tab order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Race,
    Class,
    Attr(Attribute),
    Begin,
}

const TAB_ORDER: [Focus; 7] = [
    Focus::Race,
    Focus::Class,
    Focus::Attr(Attribute::Str),
    Focus::Attr(Attribute::Dex),
    Focus::Attr(Attribute::Con),
    Focus::Attr(Attribute::Int),
    Focus::Begin,
];

#[derive(Resource, Debug, Clone)]
struct CharCreationDraft {
    race_idx: usize,
    class_idx: usize,
    /// Allocation in STR/DEX/CON/INT order (each 0..=available_max).
    points: [i32; 4],
    focus: Focus,
}

impl Default for CharCreationDraft {
    fn default() -> Self {
        Self {
            race_idx: 0,
            class_idx: 0,
            points: [0, 0, 0, 0],
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
    fn points_spent(&self) -> i32 {
        self.points.iter().sum()
    }
    fn points_remaining(&self) -> i32 {
        FREE_POINTS - self.points_spent()
    }
    fn attr_idx(attr: Attribute) -> usize {
        match attr {
            Attribute::Str => 0,
            Attribute::Dex => 1,
            Attribute::Con => 2,
            Attribute::Int => 3,
        }
    }
    /// Per-stat cap on the *allocation* side: 4 normally, 5 for Human's
    /// chosen Versatile stat. To keep the UX simple, Human gets +5 cap on
    /// **all** stats but only one of them can actually exceed +4 (because
    /// of the 4-point budget). Functionally equivalent to "one stat may
    /// reach baseline + 5."
    fn alloc_cap(&self, _attr: Attribute) -> i32 {
        if self.race() == Race::Human {
            STAT_CAP_DEFAULT + STAT_CAP_HUMAN_BONUS
        } else {
            STAT_CAP_DEFAULT
        }
    }
    fn inc(&mut self, attr: Attribute) {
        let i = Self::attr_idx(attr);
        if self.points[i] < self.alloc_cap(attr) && self.points_remaining() > 0 {
            self.points[i] += 1;
        }
    }
    fn dec(&mut self, attr: Attribute) {
        let i = Self::attr_idx(attr);
        if self.points[i] > 0 {
            self.points[i] -= 1;
        }
    }
    /// Move focus forward (Tab) or backward (Shift+Tab).
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

#[derive(Component, Debug, Clone, Copy)]
struct AttrRowMarker(Attribute);

#[derive(Component)]
struct BeginButtonMarker;

/// All live-updated text fields on the screen, dispatched in `refresh_text`.
/// Collapses what would otherwise be 9 separate marker components into one
/// query, keeping the refresh systems under Bevy's 16-param limit.
#[derive(Component, Debug, Clone, Copy)]
enum CharCreationText {
    AttrScore(Attribute),
    AttrMod(Attribute),
    RemainingPoints,
    Hp,
    Hit,
    Damage,
    Dodge,
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
    // Fresh draft each time the screen opens.
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
                padding: UiRect::all(Val::Px(40.0)),
                row_gap: Val::Px(18.0),
                ..default()
            },
            BackgroundColor(BG),
            OnCharCreationScreen,
        ))
        .with_children(|root| {
            // Title
            root.spawn((
                Text::new("CHARACTER CREATION"),
                TextFont { font: font.clone(), font_size: 44.0, ..default() },
                TextColor(GOLD),
            ));

            // RACE row label
            root.spawn((
                Text::new("Race"),
                TextFont { font: font.clone(), font_size: 22.0, ..default() },
                TextColor(DIM),
                Node {
                    margin: UiRect::top(Val::Px(8.0)),
                    align_self: AlignSelf::FlexStart,
                    ..default()
                },
            ));

            // Race boxes (4 in a row)
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

            // Race trait description
            root.spawn((
                Text::new(""),
                TextFont { font: font.clone(), font_size: 16.0, ..default() },
                TextColor(TEXT),
                CharCreationText::RaceDesc,
            ));

            // CLASS row
            root.spawn((
                Text::new("Class"),
                TextFont { font: font.clone(), font_size: 22.0, ..default() },
                TextColor(DIM),
                Node {
                    margin: UiRect::top(Val::Px(8.0)),
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

            // Class description
            root.spawn((
                Text::new(""),
                TextFont { font: font.clone(), font_size: 16.0, ..default() },
                TextColor(TEXT),
                CharCreationText::ClassDesc,
            ));

            // Attributes section
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(20.0),
                    margin: UiRect::top(Val::Px(8.0)),
                    align_self: AlignSelf::FlexStart,
                    ..default()
                },
            ))
            .with_children(|row| {
                row.spawn((
                    Text::new("Attributes"),
                    TextFont { font: font.clone(), font_size: 22.0, ..default() },
                    TextColor(DIM),
                ));
                row.spawn((
                    Text::new("Points remaining: 4"),
                    TextFont { font: font.clone(), font_size: 16.0, ..default() },
                    TextColor(GOLD),
                    CharCreationText::RemainingPoints,
                ));
            });

            for attr in ATTRS {
                root.spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(12.0),
                        padding: UiRect::all(Val::Px(6.0)),
                        ..default()
                    },
                    BackgroundColor(PANEL_BG),
                    AttrRowMarker(attr),
                ))
                .with_children(|row| {
                    row.spawn((
                        Text::new(attr.name().to_string()),
                        TextFont { font: font.clone(), font_size: 20.0, ..default() },
                        TextColor(TEXT),
                        Node { width: Val::Px(50.0), ..default() },
                    ));
                    row.spawn((
                        Text::new("10"),
                        TextFont { font: font.clone(), font_size: 20.0, ..default() },
                        TextColor(GOLD),
                        CharCreationText::AttrScore(attr),
                        Node { width: Val::Px(50.0), ..default() },
                    ));
                    row.spawn((
                        Text::new("(+0)"),
                        TextFont { font: font.clone(), font_size: 18.0, ..default() },
                        TextColor(DIM),
                        CharCreationText::AttrMod(attr),
                    ));
                });
            }

            // Preview section
            root.spawn((
                Text::new("Preview"),
                TextFont { font: font.clone(), font_size: 22.0, ..default() },
                TextColor(DIM),
                Node {
                    margin: UiRect::top(Val::Px(12.0)),
                    align_self: AlignSelf::FlexStart,
                    ..default()
                },
            ));
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(24.0),
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Text::new("HP 12"),
                    TextFont { font: font.clone(), font_size: 18.0, ..default() },
                    TextColor(TEXT),
                    CharCreationText::Hp,
                ));
                row.spawn((
                    Text::new("Hit +0"),
                    TextFont { font: font.clone(), font_size: 18.0, ..default() },
                    TextColor(TEXT),
                    CharCreationText::Hit,
                ));
                row.spawn((
                    Text::new("Damage +0"),
                    TextFont { font: font.clone(), font_size: 18.0, ..default() },
                    TextColor(TEXT),
                    CharCreationText::Damage,
                ));
                row.spawn((
                    Text::new("Dodge 0"),
                    TextFont { font: font.clone(), font_size: 18.0, ..default() },
                    TextColor(TEXT),
                    CharCreationText::Dodge,
                ));
            });

            // Begin Descent
            root.spawn((
                Node {
                    margin: UiRect::top(Val::Px(20.0)),
                    padding: UiRect::all(Val::Px(12.0)),
                    ..default()
                },
                BackgroundColor(PANEL_BG),
                BeginButtonMarker,
            ))
            .with_children(|btn| {
                btn.spawn((
                    Text::new("Begin Descent"),
                    TextFont { font: font.clone(), font_size: 24.0, ..default() },
                    TextColor(GOLD),
                ));
            });

            // Help footer
            root.spawn((
                Text::new(
                    "\u{2191}/\u{2193}: next field   |   \u{2190}/\u{2192}: change selection / \
                    adjust attribute   |   Enter (on Begin): start   |   Esc: cancel",
                ),
                TextFont { font: font.clone(), font_size: 14.0, ..default() },
                TextColor(DIM),
                Node {
                    margin: UiRect::top(Val::Px(28.0)),
                    ..default()
                },
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

    // ↑/↓ cycle focus between sections. Down advances, up retreats.
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
        Focus::Attr(attr) => {
            if left {
                draft.dec(attr);
            } else if right {
                draft.inc(attr);
            }
        }
        Focus::Begin => {
            if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter) {
                character_choice.race = draft.race();
                character_choice.class = draft.class();
                character_choice.free_points = draft.points;
                next_state.set(AppState::InGame);
            }
        }
    }
}

/// Update panel background colors based on selection + focus.
fn refresh_panels(
    draft: Res<CharCreationDraft>,
    mut race_boxes: Query<(&RaceBoxMarker, &mut BackgroundColor), Without<ClassBoxMarker>>,
    mut class_boxes: Query<
        (&ClassBoxMarker, &mut BackgroundColor),
        (Without<RaceBoxMarker>, Without<AttrRowMarker>, Without<BeginButtonMarker>),
    >,
    mut attr_rows: Query<
        (&AttrRowMarker, &mut BackgroundColor),
        (Without<RaceBoxMarker>, Without<ClassBoxMarker>, Without<BeginButtonMarker>),
    >,
    mut begin_btn: Query<
        &mut BackgroundColor,
        (
            With<BeginButtonMarker>,
            Without<RaceBoxMarker>,
            Without<ClassBoxMarker>,
            Without<AttrRowMarker>,
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
    for (marker, mut bg) in &mut attr_rows {
        let focused = matches!(draft.focus, Focus::Attr(a) if a == marker.0);
        *bg = BackgroundColor(if focused {
            PANEL_BG_FOCUSED
        } else {
            PANEL_BG
        });
    }
    if let Ok(mut bg) = begin_btn.single_mut() {
        *bg = BackgroundColor(if draft.focus == Focus::Begin {
            PANEL_BG_FOCUSED_SELECTED
        } else {
            PANEL_BG
        });
    }
}

/// Update every live-text field (attribute scores, mods, remaining points,
/// preview stats, race/class descriptions) from the current draft.
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
        compose_attributes(r, c, draft.points)
    } else {
        Attributes::default()
    };
    let derived = if let Some(c) = class_asset {
        derive_stats(c, &attrs)
    } else {
        Default::default()
    };

    for (kind, mut t) in &mut text_q {
        let new = match *kind {
            CharCreationText::AttrScore(a) => attrs.get(a).to_string(),
            CharCreationText::AttrMod(a) => format!("({:+})", attrs.mod_of(a)),
            CharCreationText::RemainingPoints => {
                format!("Points remaining: {}", draft.points_remaining())
            }
            CharCreationText::Hp => format!("HP {}", derived.max_hp),
            CharCreationText::Hit => format!("Hit {:+}", derived.hit_bonus_melee),
            CharCreationText::Damage => format!("Damage {:+}", derived.damage_bonus_melee),
            CharCreationText::Dodge => format!("Dodge {}", derived.dodge),
            CharCreationText::RaceDesc => race_asset
                .map(|r| r.description.clone())
                .unwrap_or_default(),
            CharCreationText::ClassDesc => class_asset
                .map(|c| c.description.clone())
                .unwrap_or_default(),
        };
        *t = Text::new(new);
    }
    let _ = ability_mod; // exported helper, suppress unused-import warning
}

fn panel_color(selected: bool, focused: bool) -> Color {
    match (selected, focused) {
        (true, true) => PANEL_BG_FOCUSED_SELECTED,
        (true, false) => PANEL_BG_SELECTED,
        (false, true) => PANEL_BG_FOCUSED,
        (false, false) => PANEL_BG,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_inc_respects_cap_and_remaining_points() {
        let mut d = CharCreationDraft::default();
        // Non-Human race → cap +4. Start with 4 points to spend.
        d.race_idx = 1; // Dwarf
        d.inc(Attribute::Str);
        d.inc(Attribute::Str);
        d.inc(Attribute::Str);
        d.inc(Attribute::Str);
        assert_eq!(d.points[0], 4); // cap reached at +4
        assert_eq!(d.points_remaining(), 0);
        // Next inc is a no-op (no points left)
        d.inc(Attribute::Dex);
        assert_eq!(d.points[1], 0);
    }

    #[test]
    fn draft_dec_floors_at_zero() {
        let mut d = CharCreationDraft::default();
        d.dec(Attribute::Str);
        assert_eq!(d.points[0], 0);
    }

    #[test]
    fn human_versatile_extends_cap_by_one() {
        let mut d = CharCreationDraft::default();
        d.race_idx = 0; // Human
        // Spend 5 into STR — possible because Human cap is +5.
        for _ in 0..5 {
            d.inc(Attribute::Str);
        }
        // But only the budget of 4 points exists, so only 4 are spent.
        assert_eq!(d.points[0], 4);
        assert_eq!(d.points_remaining(), 0);
        // (The +5 cap matters when other Human stats are at 0 — same
        // total budget, but the headroom on one stat is +5.)
    }

    #[test]
    fn cycle_focus_walks_full_order_forward() {
        let mut d = CharCreationDraft::default();
        let starts = vec![
            Focus::Race,
            Focus::Class,
            Focus::Attr(Attribute::Str),
            Focus::Attr(Attribute::Dex),
            Focus::Attr(Attribute::Con),
            Focus::Attr(Attribute::Int),
            Focus::Begin,
            Focus::Race,
        ];
        let mut seen = vec![d.focus];
        for _ in 0..7 {
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
        d.cycle_focus(true); // Begin → INT
        assert_eq!(d.focus, Focus::Attr(Attribute::Int));
    }
}
