---
name: bevy-single-query
description: Mandatory use of single() and single_mut() for unique entity queries. get_single() and get_single_mut() are unavailable in this environment and will cause compilation errors. Use when accessing guaranteed singleton entities like the Player or UI roots.
---

# Bevy Single Query Access

## Overview

In this project, `get_single()` and `get_single_mut()` **do not exist** and will cause compilation errors. You must exclusively use `single()` and `single_mut()` when querying for unique entities.

This applies to any query where the system logic depends on the existence of exactly one entity (singletons).

## Guidelines

1.  **Mandatory `single()`**: Use for read-only access to components on singleton entities.
2.  **Mandatory `single_mut()`**: Use for mutable access to components on singleton entities.
3.  **No Fallbacks**: Do not attempt to use `get_single` or result-based handling for these queries; the compiler will reject it.

## Common Singleton Targets

*   **Player**: `query_player.single()`
*   **Cameras**: `q_ui_camera.single()`, `q_main_camera.single()`
*   **UI Roots**: `q_tooltip_root.single_mut()`, `q_log_root.single()`

## Example

### Correct Implementation

```rust
fn update_player_stats(
    player_query: Query<&Health, With<Player>>,
    mut text_query: Query<&mut Text, With<PlayerHealthText>>,
) {
    // Correct: Direct access
    let health = player_query.single();
    let mut text = text_query.single_mut();
    
    text.0 = format!("HP: {}/{}", health.current, health.max);
}
```

### Incorrect Implementation (Will Fail to Compile)

```rust
fn update_player_stats(
    player_query: Query<&Health, With<Player>>,
    mut text_query: Query<&mut Text, With<PlayerHealthText>>,
) {
    // ERROR: get_single() does not exist
    if let Ok(health) = player_query.get_single() { 
        // ...
    }
}
```
