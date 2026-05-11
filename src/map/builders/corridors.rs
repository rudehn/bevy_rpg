pub use roguelike_engine::map::builders::corridors::{NearestCorridors, draw_corridor};

use crate::map::builders::{BuilderMap, MetaMapBuilder};

impl MetaMapBuilder for NearestCorridors {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        use roguelike_engine::map::builders::MapBuilder;
        self.build(build_data);
    }
}
