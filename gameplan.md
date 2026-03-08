
Add light source, blocked by walls
Add floors & allow player to go up and down floors
add spawner weight

assets
https://www.oryxdesignlab.com/products/p/tiny-dungeon-tileset
https://www.oryxdesignlab.com/products/p/ultimate-fantasy-tileset


# Bugs
* Light extends past walls and outside the map

# Features
* Add support for going back up levels

# Todo
* Refactor candle & goblin spawning from being hard coded in spawn dungeon function
* make a visibility file??
* Remove floor resource in favor of Map.depth
* Any better way to despawn tilemap & children entities?
* Any better way to indicate which floor entities are on?
* Revisit player_spawn_or_move_system, see other branch??
* Update tile visiblity after player moved?? - may not be needed
* Add hordes
* WASM
particle effects
* Save game
* Load game from Menu

add different attack names

Add tests
- Skill for adding tests?
- Skill for auto running tests?

Fix character status line not despawned on game over
Add better sprites for:
- downstairs

Are we rendering tiles twice? With sprites & ecs_tilemap (see update_tile_visibility)