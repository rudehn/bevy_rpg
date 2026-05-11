pub use roguelike_engine::map::builders::diagonal_culler::DiagonalCuller;

use crate::map::builders::{BuilderMap, MetaMapBuilder};

impl MetaMapBuilder for DiagonalCuller {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        use roguelike_engine::map::builders::MapBuilder;
        self.build(build_data);
    }
}
