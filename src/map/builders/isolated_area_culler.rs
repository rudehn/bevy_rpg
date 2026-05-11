pub use roguelike_engine::map::builders::isolated_area_culler::IsolatedAreaCuller;

use crate::map::builders::{BuilderMap, BuilderPhase, MetaMapBuilder};

impl MetaMapBuilder for IsolatedAreaCuller {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        use roguelike_engine::map::builders::MapBuilder;
        self.build(build_data);
    }

    fn phase(&self) -> Option<BuilderPhase> {
        Some(BuilderPhase::ConnectivityCull)
    }
}
