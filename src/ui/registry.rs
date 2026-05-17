//! UI screen registry: declarative metadata + central dispatch for the
//! `InGameState`-owning screens.
//!
//! ## What it does
//!
//! Each modal screen (Inventory, CharacterInfo, SkillScreen, Help,
//! LogHistory, EnchantSelect, ChasmConfirm, AsiSelect) implements
//! [`UiScreen`] with a few constants and a `build(app)` method. The
//! [`RegisterScreen::register_screen`] extension on [`App`] wires the
//! screen's systems and records its hotkey + help entry in the
//! [`ScreenRegistry`] resource.
//!
//! A single exclusive [`dispatch_screen_hotkeys`] system in `Update`
//! reads the registry, checks pressed keys against each entry's key +
//! modifiers + optional gate predicate, and transitions
//! `NextState<InGameState>` on a match. A startup system
//! [`detect_screen_key_collisions`] panics if two screens claim the
//! same (key, modifiers).
//!
//! ## What the registry does NOT own
//!
//! - In-screen navigation (j/k, Escape, etc.) stays in each screen's
//!   own Update systems. Use [`close_on_toggle_or_escape`] as a generic
//!   "close on Escape or the screen's own hotkey" helper.
//! - Event-driven screens (AsiSelect, EnchantSelect, ChasmConfirm) set
//!   `OPEN_KEY: None` and have gameplay code transition into them via
//!   `NextState`.
//! - Non-stateful overlays (CheatMenu via resource flag) and AppState-
//!   level screens (pause/menu/game-over) are out of scope.

use bevy::prelude::*;

use crate::game::turns::TurnState;
use crate::game::{AppState, InGameState};

// =====================================================================
// Modifier mask
// =====================================================================

/// Modifier keys that must be held for a screen's hotkey to fire.
/// `shift: true` matches either `ShiftLeft` or `ShiftRight`; same for
/// `ctrl` and `alt`.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

impl Modifiers {
    /// No modifiers required.
    pub const NONE: Self = Self { shift: false, ctrl: false, alt: false };
    /// Shift required (matches either ShiftLeft or ShiftRight).
    pub const SHIFT: Self = Self { shift: true, ctrl: false, alt: false };

    /// True iff the current `ButtonInput<KeyCode>` state satisfies the
    /// mask: every `true` field has the corresponding modifier pressed,
    /// every `false` field has the corresponding modifier released.
    pub fn matches(&self, keys: &ButtonInput<KeyCode>) -> bool {
        let shift_held = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
        let ctrl_held = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
        let alt_held = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);
        shift_held == self.shift && ctrl_held == self.ctrl && alt_held == self.alt
    }
}

// =====================================================================
// Registry entries
// =====================================================================

/// A registered hotkey that opens a screen from `InGameState::Running`.
/// Not `Copy` because `InGameState` is `Clone` only.
#[derive(Clone, Debug)]
pub struct HotkeyEntry {
    pub state: InGameState,
    pub key: KeyCode,
    pub modifiers: Modifiers,
    /// Optional predicate evaluated against `&World` before the
    /// transition fires. Used by Inventory et al. to require
    /// `TurnState::PlayerInput`.
    pub gate: Option<fn(&World) -> bool>,
}

/// A help-screen row. Both fields are `&'static str` to keep the
/// registry allocation-free; `display` is whatever the screen author
/// wants the user to see ("I", "?", "Shift+M").
#[derive(Copy, Clone, Debug)]
pub struct HelpEntry {
    pub display: &'static str,
    pub label: &'static str,
}

// =====================================================================
// UiScreen trait
// =====================================================================

/// Declarative description of a modal `InGameState` screen.
///
/// Implementors set the metadata constants and provide a `build(app)`
/// method that registers the screen's own systems (OnEnter spawn, OnExit
/// despawn, Update for in-screen input). The trait is wired in via
/// [`RegisterScreen::register_screen`] on `App`.
pub trait UiScreen: Send + Sync + 'static {
    /// The `InGameState` variant this screen owns.
    const STATE: InGameState;
    /// Hotkey that opens the screen from `Running`. `None` for
    /// event-driven screens (AsiSelect, EnchantSelect, ChasmConfirm).
    const OPEN_KEY: Option<KeyCode>;
    /// Required modifier mask for the hotkey. Defaults to none.
    const OPEN_MODIFIERS: Modifiers = Modifiers::NONE;
    /// Optional gate predicate. The dispatcher runs this with `&World`
    /// before transitioning; returning `false` blocks the open.
    const OPEN_GATE: Option<fn(&World) -> bool> = None;
    /// Optional help-screen row. `None` for screens hidden from help.
    const HELP: Option<HelpEntry> = None;

    /// Wire the screen's own systems into the app. Typically:
    /// `OnEnter(STATE) -> spawn`, `OnExit(STATE) -> despawn_screen::<Marker>`,
    /// and `Update` systems gated on `in_state(STATE)` for in-screen
    /// input / refresh.
    fn build(app: &mut App);
}

// =====================================================================
// Registry resource
// =====================================================================

/// Resource collecting every registered screen's hotkey + help entry.
/// Populated by [`RegisterScreen::register_screen`] at app build time.
#[derive(Resource, Default, Clone, Debug)]
pub struct ScreenRegistry {
    pub hotkeys: Vec<HotkeyEntry>,
    pub help_entries: Vec<HelpEntry>,
}

impl ScreenRegistry {
    /// Find the first pair of hotkey entries that share the same
    /// `(key, modifiers)`. Returns `Some((i, j))` with `i < j` on
    /// collision, `None` otherwise.
    pub fn find_collision(&self) -> Option<(usize, usize)> {
        for i in 0..self.hotkeys.len() {
            for j in (i + 1)..self.hotkeys.len() {
                let a = &self.hotkeys[i];
                let b = &self.hotkeys[j];
                if a.key == b.key && a.modifiers == b.modifiers {
                    return Some((i, j));
                }
            }
        }
        None
    }
}

// =====================================================================
// App extension
// =====================================================================

/// Extension trait on `App` for registering screens.
pub trait RegisterScreen {
    /// Wire `S::build(app)` and record `S`'s hotkey + help entry in the
    /// `ScreenRegistry`. Call once per screen in `UiPlugin::build`.
    fn register_screen<S: UiScreen>(&mut self) -> &mut Self;
}

impl RegisterScreen for App {
    fn register_screen<S: UiScreen>(&mut self) -> &mut Self {
        S::build(self);
        let mut reg = self.world_mut().resource_mut::<ScreenRegistry>();
        if let Some(key) = S::OPEN_KEY {
            reg.hotkeys.push(HotkeyEntry {
                state: S::STATE,
                key,
                modifiers: S::OPEN_MODIFIERS,
                gate: S::OPEN_GATE,
            });
        }
        if let Some(help) = S::HELP {
            reg.help_entries.push(help);
        }
        self
    }
}

// =====================================================================
// Dispatch
// =====================================================================

/// Exclusive system: scan the registry for a hotkey match and
/// transition to the matched screen. Runs only in
/// `InGameState::Running` (modulo `AppState::InGame`).
pub fn dispatch_screen_hotkeys(world: &mut World) {
    // First pass: read-only, find a matching entry.
    let mut chosen: Option<(InGameState, Option<fn(&World) -> bool>)> = None;
    {
        let keys = world.resource::<ButtonInput<KeyCode>>();
        let registry = world.resource::<ScreenRegistry>();
        for entry in registry.hotkeys.iter() {
            if keys.just_pressed(entry.key) && entry.modifiers.matches(keys) {
                chosen = Some((entry.state.clone(), entry.gate));
                break;
            }
        }
    }
    let Some((target, gate)) = chosen else { return; };

    // Optional gate predicate has full World read access.
    if let Some(g) = gate {
        if !g(&*world) {
            return;
        }
    }

    world.resource_mut::<NextState<InGameState>>().set(target);
}

/// Startup system: panic if two registered screens share the same
/// `(key, modifiers)`. Caught at first boot rather than during
/// playtest.
pub fn detect_screen_key_collisions(registry: Res<ScreenRegistry>) {
    if let Some((i, j)) = registry.find_collision() {
        let a = &registry.hotkeys[i];
        let b = &registry.hotkeys[j];
        panic!(
            "Screen hotkey collision: {:?} ({:?}+{:?}) is bound to both {:?} and {:?}",
            a.key, a.modifiers, b.modifiers, a.state, b.state
        );
    }
}

// =====================================================================
// Standard close-on-toggle-or-escape helper
// =====================================================================

/// Shared `OPEN_GATE` predicate: only allow opening the screen during
/// the player's turn. Used by Inventory, CharacterInfo, SkillScreen,
/// LogHistory. The check is just "is `TurnState == PlayerInput`?".
pub fn open_gate_player_turn(world: &World) -> bool {
    matches!(world.resource::<State<TurnState>>().get(), TurnState::PlayerInput)
}

/// Generic close-input system for the common case: pressing the
/// screen's own hotkey (with modifiers) or Escape returns to
/// `InGameState::Running`. Screens with custom close logic (auto-close
/// when state is exhausted, multi-step wizards, etc.) should skip this
/// and write their own input system.
pub fn close_on_toggle_or_escape<S: UiScreen>(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<InGameState>>,
    mut next: ResMut<NextState<InGameState>>,
) {
    if *state.get() != S::STATE {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        next.set(InGameState::Running);
        return;
    }
    if let Some(key) = S::OPEN_KEY {
        if keys.just_pressed(key) && S::OPEN_MODIFIERS.matches(&keys) {
            next.set(InGameState::Running);
        }
    }
}

// =====================================================================
// Plugin
// =====================================================================

/// Plugin that initialises [`ScreenRegistry`] and wires the dispatch +
/// collision-detection systems. Add this BEFORE registering any
/// screens (typically the first call in `UiPlugin::build`).
pub struct ScreenRegistryPlugin;

impl Plugin for ScreenRegistryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScreenRegistry>()
            .add_systems(Startup, detect_screen_key_collisions)
            .add_systems(
                Update,
                dispatch_screen_hotkeys
                    .run_if(in_state(AppState::InGame))
                    .run_if(in_state(InGameState::Running)),
            );
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn input_with(pressed: &[KeyCode]) -> ButtonInput<KeyCode> {
        let mut input = ButtonInput::<KeyCode>::default();
        for &key in pressed {
            input.press(key);
        }
        input
    }

    // ----- Modifiers::matches -----

    #[test]
    fn modifiers_none_matches_no_modifiers_held() {
        let input = input_with(&[KeyCode::KeyI]);
        assert!(Modifiers::NONE.matches(&input));
    }

    #[test]
    fn modifiers_none_rejects_any_modifier_held() {
        let input = input_with(&[KeyCode::KeyI, KeyCode::ShiftLeft]);
        assert!(!Modifiers::NONE.matches(&input));
    }

    #[test]
    fn modifiers_shift_matches_shift_left() {
        let input = input_with(&[KeyCode::Slash, KeyCode::ShiftLeft]);
        assert!(Modifiers::SHIFT.matches(&input));
    }

    #[test]
    fn modifiers_shift_matches_shift_right() {
        let input = input_with(&[KeyCode::Slash, KeyCode::ShiftRight]);
        assert!(Modifiers::SHIFT.matches(&input));
    }

    #[test]
    fn modifiers_shift_rejects_no_shift_held() {
        let input = input_with(&[KeyCode::Slash]);
        assert!(!Modifiers::SHIFT.matches(&input));
    }

    #[test]
    fn modifiers_shift_rejects_ctrl_held_too() {
        // Asks for shift only; ctrl is extra → reject.
        let input = input_with(&[KeyCode::Slash, KeyCode::ShiftLeft, KeyCode::ControlLeft]);
        assert!(!Modifiers::SHIFT.matches(&input));
    }

    #[test]
    fn modifiers_custom_combination() {
        let mods = Modifiers { shift: true, ctrl: true, alt: false };
        let exact = input_with(&[KeyCode::ShiftLeft, KeyCode::ControlLeft]);
        assert!(mods.matches(&exact));
        let missing_ctrl = input_with(&[KeyCode::ShiftLeft]);
        assert!(!mods.matches(&missing_ctrl));
        let extra_alt = input_with(&[KeyCode::ShiftLeft, KeyCode::ControlLeft, KeyCode::AltLeft]);
        assert!(!mods.matches(&extra_alt));
    }

    // ----- ScreenRegistry::find_collision -----

    fn entry(state: InGameState, key: KeyCode, modifiers: Modifiers) -> HotkeyEntry {
        HotkeyEntry { state, key, modifiers, gate: None }
    }

    #[test]
    fn collision_detection_empty_registry() {
        let reg = ScreenRegistry::default();
        assert_eq!(reg.find_collision(), None);
    }

    #[test]
    fn collision_detection_single_entry() {
        let reg = ScreenRegistry {
            hotkeys: vec![entry(InGameState::Inventory, KeyCode::KeyI, Modifiers::NONE)],
            help_entries: vec![],
        };
        assert_eq!(reg.find_collision(), None);
    }

    #[test]
    fn collision_detection_distinct_keys() {
        let reg = ScreenRegistry {
            hotkeys: vec![
                entry(InGameState::Inventory, KeyCode::KeyI, Modifiers::NONE),
                entry(InGameState::CharacterInfo, KeyCode::KeyC, Modifiers::NONE),
            ],
            help_entries: vec![],
        };
        assert_eq!(reg.find_collision(), None);
    }

    #[test]
    fn collision_detection_same_key_different_modifiers_is_not_collision() {
        // Slash with no mods (Inventory hypothetical) and Slash+Shift (Help) coexist fine.
        let reg = ScreenRegistry {
            hotkeys: vec![
                entry(InGameState::Inventory, KeyCode::Slash, Modifiers::NONE),
                entry(InGameState::Help, KeyCode::Slash, Modifiers::SHIFT),
            ],
            help_entries: vec![],
        };
        assert_eq!(reg.find_collision(), None);
    }

    #[test]
    fn collision_detection_exact_duplicate() {
        let reg = ScreenRegistry {
            hotkeys: vec![
                entry(InGameState::Inventory, KeyCode::KeyI, Modifiers::NONE),
                entry(InGameState::CharacterInfo, KeyCode::KeyI, Modifiers::NONE),
            ],
            help_entries: vec![],
        };
        assert_eq!(reg.find_collision(), Some((0, 1)));
    }

    #[test]
    fn collision_detection_reports_first_pair() {
        // Three screens, second + third collide. Should report (1, 2).
        let reg = ScreenRegistry {
            hotkeys: vec![
                entry(InGameState::Inventory, KeyCode::KeyI, Modifiers::NONE),
                entry(InGameState::CharacterInfo, KeyCode::KeyM, Modifiers::NONE),
                entry(InGameState::SkillScreen, KeyCode::KeyM, Modifiers::NONE),
            ],
            help_entries: vec![],
        };
        assert_eq!(reg.find_collision(), Some((1, 2)));
    }
}
