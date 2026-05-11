pub use roguelike_engine::map::builders::start_point::{StartPointBuilder, XStart, YStart};

// Legacy adapter: the game's BuilderChain still uses MetaMapBuilder
use crate::map::builders::{BuilderMap, MetaMapBuilder};

impl MetaMapBuilder for StartPointBuilder {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        use roguelike_engine::map::builders::MapBuilder;
        self.build(build_data);
    }
}
