---
name: clean-deserialization
description: Enforce clean RON deserialization patterns for optional fields in game assets. Use this skill when adding or modifying fields in MonsterAsset or other RON-backed data structures to ensure a consistent, non-wrapped syntax in asset files.
---

# Clean Deserialization

## Overview

This skill ensures that optional fields in game assets (like `MonsterAsset` in `src/assets/mod.rs`) are deserialized in a way that allows them to be omitted or provided as raw values in `.ron` files, avoiding the need for `Some()` or `None` wrappers.

## Guidelines

When adding an optional field to a struct that is backed by a `.ron` file:

1.  **Use the Helper**: Apply the `#[serde(default, deserialize_with = "...")]` attribute to the field.
2.  **Match the Type**: Use the appropriate helper function from `serde_helpers` in `src/assets/mod.rs`:
    *   `f32` -> `serde_helpers::deserialize_f32_as_option`
    *   `i32` -> `serde_helpers::deserialize_i32_as_option`
3.  **RON Syntax**: Ensure the resulting `.ron` file uses raw values (e.g., `regen: 20` instead of `regen: Some(20)`) and omit the field entirely if the value is not needed.

## Examples

### Correct Component Definition

```rust
#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct MonsterAsset {
    pub name: String,
    // ... other fields
    #[serde(default, deserialize_with = "serde_helpers::deserialize_i32_as_option")]
    pub regen: Option<i32>,
}
```

### Correct RON Syntax

```ron
(
    monsters: {
        "Orc": (
            name: "Orc",
            // ... other fields
            regen: 20, // Clean value
        ),
        "Goblin": (
            name: "Goblin",
            // regen is omitted entirely
        ),
    }
)
```

## Implementation Reference

All deserialization helpers are defined in `src/assets/mod.rs` within the `serde_helpers` module. If a new type is needed, add a corresponding helper there.
