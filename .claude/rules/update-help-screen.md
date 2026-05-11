# Update Help Screen When Keybindings Change

When adding, removing, or changing any keybinding:
1. Update `src/ui/help.rs` `spawn_help_ui()` to reflect the new binding
2. Add new bindings to the appropriate section (Movement, Screens, Inventory Actions, Targeting, Camera)
3. If a new section is needed, add it using the `section()` helper pattern
4. Remove bindings that no longer exist
