pub use roguelike_engine::map::builders::finish_doors::FinishDoors;

use crate::map::builders::{BuilderMap, MetaMapBuilder};

impl MetaMapBuilder for FinishDoors {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        use roguelike_engine::map::builders::MapBuilder;
        self.build(build_data);
    }
}
