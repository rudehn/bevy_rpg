pub use roguelike_engine::map::builders::bsp_dungeon::{BspConfig, BspDungeonBuilder};

use crate::map::builders::{BuilderMap, InitialMapBuilder};

impl InitialMapBuilder for BspDungeonBuilder {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        use roguelike_engine::map::builders::MapBuilder;
        self.build(build_data);
    }
}
