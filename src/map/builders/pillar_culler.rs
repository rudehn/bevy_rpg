pub use roguelike_engine::map::builders::pillar_culler::PillarCuller;

use crate::map::builders::{BuilderMap, MetaMapBuilder};

impl MetaMapBuilder for PillarCuller {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        use roguelike_engine::map::builders::MapBuilder;
        self.build(build_data);
    }
}
