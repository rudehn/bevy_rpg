Add map generation
Add player and movement
Add light source, blocked by walls
Add player vision
Add floors & allow player to go up and down floors


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
* On death, go to end game state, then go to main menu and clear map assets
* Add hordes
* WASM
* Speed up frame rate

* Save game
* Load game from Menu
