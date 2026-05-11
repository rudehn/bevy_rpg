pub use roguelike_engine::map::builders::lake_builder::LakeBuilder;

use crate::map::builders::{BuilderMap, MetaMapBuilder};

impl MetaMapBuilder for LakeBuilder {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        use roguelike_engine::map::builders::MapBuilder;
        self.build(build_data);
    }
}
