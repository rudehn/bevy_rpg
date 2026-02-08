# Bevy RPG Game Project

## Project Overview

`bevy_rpg` is a 2D roguelike game built with the Bevy game engine. It aims to feature procedurally generated dungeons, player movement, a realistic field-of-view (FOV) system based on light and opaque walls, and support for multiple dungeon floors. The project leverages `bevy_ecs_tilemap` for efficient tile-based rendering and `bracket-lib` for roguelike-specific functionalities like FOV calculations and dungeon generation algorithms.

## Main Technologies

*   **Rust:** The primary programming language.
*   **Bevy Engine:** A data-driven game engine (version `0.17.0`).
*   **bevy_ecs_tilemap:** A Bevy plugin for tilemap rendering (version `0.17.0`).
*   **bracket-lib:** A utility library for roguelike development, used for FOV and dungeon generation.
*   **bevy_light_2d:** A Bevy plugin for 2D lighting (version `0.8.0`), though its usage appears commented out in some parts of the codebase, suggesting it might be a planned or partially integrated feature.
*   **petgraph:** A graph data structure library (version `0.8.3`), likely used for pathfinding or dungeon generation algorithms.
*   **rand:** A random number generation library (version `0.9.2`), essential for procedural content generation.

## Architecture Highlights

*   **Bevy ECS Model:** The game follows Bevy's Entity-Component-System (ECS) architectural pattern, centralizing game logic and data management.
*   **Core Application Setup (`src/main.rs`):** Initializes Bevy's default plugins, image rendering with `ImagePlugin::default_nearest()`, and integrates custom game-specific plugins: `LoadingPlugin`, `GamePlugin`, and `MenuPlugin`. Game states are managed using a custom `AppState` enum.
*   **Map Management (`src/map/map.rs`):** Contains the `MapPlugin` responsible for setting up `bevy_ecs_tilemap`, handling dungeon spawn messages (`SpawnDungeonMessage`), and managing tile visibility. The `spawn_dungeon` system orchestrates the dungeon generation using `level_builder` and spawns individual tile entities. This file also defines `GameMap` and `EcsMap` structs which implement `bracket-lib`'s `BaseMap` and `Algorithm2D` traits, enabling robust map interaction and pathfinding.
*   **Tile Definitions (`src/map/tile.rs`):** Defines the various `TileType`s (e.g., `Wall`, `Floor`, `DownStairs`), their physical properties (`is_walkable`, `is_opaque`), and utility functions like `tile_texture`. The `spawn_tile_entity` function handles the creation of individual tile entities with appropriate Bevy components, including `Collider` for non-walkable tiles.
*   **Game Systems (`src/game/systems.rs`):** Houses core game logic, such as the `fov_update_system` which dynamically calculates and updates the player's field of view based on map opacity.
*   **Modular Design:** The project is organized into logical modules (`game`, `map`, `player`, `menu`, `components`, `constants`) to enhance maintainability and separation of concerns.

## Building and Running

The project uses Cargo, the Rust package manager and build system.

*   **Build the project:**
    ```bash
    cargo build
    ```
*   **Run the game:**
    ```bash
    cargo run
    ```
*   **Check for compilation errors without building an executable:**
    ```bash
    cargo check
    ```
*   **Automatically fix simple linter warnings (e.g., unused imports, variables):**
    ```bash
    cargo fix --bin bevy_rpg
    ```

## Development Conventions

*   **Bevy ECS Best Practices:** Adherence to the Bevy engine's Entity-Component-System (ECS) architecture is a core convention.
*   **Modularity:** Code is logically separated into distinct modules, each with a clear responsibility. Prefer using messages rather than ordering systems to keep systems decoupled
*   **Readability:** Functions are generally concise and focused on single responsibilities, as demonstrated by the `spawn_tile_entity` function.
*   **`bracket-lib` Integration:** Extensive use and implementation of `bracket-lib`'s map-related traits (`BaseMap`, `Algorithm2D`) for roguelike mechanics.
