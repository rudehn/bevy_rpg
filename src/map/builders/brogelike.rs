pub use roguelike_engine::map::builders::brogelike::BrogueLikeBuilder;

use crate::map::builders::{BuilderMap, InitialMapBuilder};

impl InitialMapBuilder for BrogueLikeBuilder {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        use roguelike_engine::map::builders::MapBuilder;
        self.build(build_data);
    }
}
