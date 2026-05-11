pub use roguelike_engine::map::builders::cave_eroder::CaveEroder;

use crate::map::builders::{BuilderMap, MetaMapBuilder};

impl MetaMapBuilder for CaveEroder {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        use roguelike_engine::map::builders::MapBuilder;
        self.build(build_data);
    }
}
