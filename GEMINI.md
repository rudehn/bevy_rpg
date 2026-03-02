# Bevy RPG Game Project

## Project Overview

`bevy_rpg` is a 2D roguelike game built with the Bevy game engine (v0.17.0). It features procedurally generated dungeons, player movement, field-of-view (FOV) based on light and opaque walls, and support for multiple dungeon floors. The project integrates `bevy_ecs_tilemap` for tile rendering and a forked version of `bracket-lib` for roguelike-specific algorithms.

## Main Technologies

*   **Rust:** Primary programming language (Edition 2024).
*   **Bevy Engine (0.17.0):** Data-driven game engine.
*   **bevy_ecs_tilemap (0.17.0):** Optimized tilemap rendering.
*   **bracket-lib (forked):** Used for FOV calculations, pathfinding, and dungeon generation.
*   **bevy_light_2d (0.8.0):** Integrated for 2D lighting effects.
*   **bevy_common_assets:** Used for loading RON files for monster and spawn manifests.
*   **serde:** Serialization/deserialization for assets.
*   **bevy_save (0.17.0):** Potential for game state persistence.

## Architecture Highlights

*   **State Management:** The game uses an `AppState` enum: `Loading`, `Menu`, `InGame`, and `GameOver`.
*   **Dual Map System:**
    *   **Logic Map (`Map`):** A grid-based struct implementing `bracket-lib`'s `BaseMap` and `Algorithm2D`. It handles collision, opacity, and pathing.
    *   **Rendering Map (`bevy_ecs_tilemap`):** Handles the visual representation, visibility (fog of war), and pixel locations.
*   **Dungeon Generation:** Implemented via a **Builder Chain** pattern (`src/map/builders/mod.rs`). This allows composing different algorithms (BSP, Room Drawer, Corridor Builders) to create levels.
*   **Field of View (FOV):** The `Viewshed` component stores visible tiles, which are updated by the `fov_update_system` using `bracket-lib`'s FOV algorithms. Monster visibility and tile color (fog of war) are synced with the viewshed.
*   **Asset Management:** Centralized loading in `src/assets/mod.rs`. Monster definitions and spawn tables are loaded from `.ron` files in the `assets/` directory.

## Core Modules

*   `src/main.rs`: Application entry point and plugin setup.
*   `src/map/`: Map logic, tile definitions, and dungeon generation builders.
*   `src/game/`: Core game systems (FOV, monster visibility, turn management, combat, AI).
*   `src/player/`: Player-specific logic and input handling.
*   `src/components.rs`: Shared ECS components (`Position`, `Viewshed`, `Monster`, etc.).
*   `src/assets/`: Asset loading and manifest definitions.

## Building and Running

*   **Run the game:**
    ```bash
    cargo run
    ```
*   **Build the project:**
    ```bash
    cargo build
    ```
*   **Check for compilation errors:**
    ```bash
    cargo check
    ```

## Development Conventions

*   **ECS First:** Strictly adhere to Bevy's ECS architecture.
*   **Modularity:** Keep systems decoupled. Use `Position` for grid coordinates and sync to `Transform` using `sync_entity_transforms`.
*   **Visibility:** Tile visibility and "explored" state are tracked via `TileVisibility` and `TileExplored` components.
*   **Resource Usage:** Use Bevy `Resource` for global state (e.g., the active `Map`, `TurnManager`).
*   **Dungeon Building:** New dungeon types should be added as `InitialMapBuilder` or `MetaMapBuilder` implementations in `src/map/builders/`.
