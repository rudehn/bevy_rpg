//! Places the player starting position and optional UpStairs.

use bevy::prelude::warn;

use super::{BuildContext, BuilderPhase, MapBuilder};
use crate::components::Position;
use crate::map::tile::{LiquidType, TerrainType};

#[derive(Clone)]
pub enum XStart { LEFT, CENTER, RIGHT }

#[derive(Clone)]
pub enum YStart { TOP, CENTER, BOTTOM }

#[derive(Clone)]
pub struct StartPointBuilder;

impl StartPointBuilder {
    pub fn new() -> Self {
        StartPointBuilder
    }
}

impl<C: BuildContext> MapBuilder<C> for StartPointBuilder {
    fn name(&self) -> &'static str {
        "StartPoint"
    }
    fn phase(&self) -> Option<BuilderPhase> {
        Some(BuilderPhase::TerrainCleanup)
    }
    fn build(&mut self, ctx: &mut C) {
        let rooms = match ctx.rooms() {
            Some(r) => r.clone(),
            None => {
                warn!("StartPointBuilder: rooms not set — skipping");
                return;
            }
        };
        let Some(first_room) = rooms.first() else {
            warn!("StartPointBuilder: rooms list is empty — skipping");
            return;
        };
        let start_pos = first_room.center();
        ctx.set_starting_position(Position {
            x: start_pos.x,
            y: start_pos.y,
        });
        if ctx.map().depth() > 1 {
            ctx.map_mut().set_tile(start_pos, TerrainType::UpStairs);
            ctx.map_mut().set_liquid(start_pos, LiquidType::None);
            bevy::log::info!(
                "StartPointBuilder: placed UpStairs at ({}, {}) on floor {}",
                start_pos.x, start_pos.y, ctx.map().depth()
            );
        }
    }
}
