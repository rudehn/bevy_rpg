use crate::{
    components::{Position, Viewshed},
    game::actions::{Direction, MeleeIntent, MovementIntent, WaitIntent},
    map::{Map, tile::is_walkable},
    player::Player,
};
use bevy::prelude::*;
use bracket_lib::prelude::{Algorithm2D, Point, a_star_search};
use rand::rng;
use rand::seq::SliceRandom;

#[derive(Component)]
pub struct Actor {
    pub ai: Box<dyn ActorAI>,
}

pub trait ActorAI: Send + Sync {
    /// AI now directly sends events to the world instead of returning an Action enum.
    fn execute(&mut self, entity: Entity, world: &mut World);
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
enum MonsterAIMode {
    #[default]
    Asleep,
    Hunting,
    Wandering,
}

#[derive(Default, Component)]
pub struct MonsterAI {
    mode: MonsterAIMode,
    last_known_player_position: Option<Point>,
}

impl MonsterAI {
    pub fn execute(&mut self, entity: Entity, world: &mut World) {
        let mut rng = rng();

        // --- STEP 1: READ-ONLY DATA EXTRACTION ---
        // We do all our World queries in a single block to ensure borrows are dropped immediately.
        let (monster_pos, monster_viewshed, player_point) = {
            let m_pos = world.get::<Position>(entity).map(|p| p.to_point());
            let m_view = world.get::<Viewshed>(entity).cloned().unwrap_or_default();

            let mut player_query = world.query_filtered::<&Position, With<Player>>();
            let p_pt = player_query.iter(world).next().map(|p| p.to_point());

            (m_pos, m_view, p_pt)
        };

        // Guard clauses
        let Some(monster_pos) = monster_pos else {
            return;
        };
        let Some(player_point) = player_point else {
            world.write_message(WaitIntent { entity });
            return;
        };

        let is_player_visible = monster_viewshed.visible_tiles.contains(&player_point);

        // --- STEP 2: STATE LOGIC (No World access) ---
        match self.mode {
            MonsterAIMode::Asleep => {
                if is_player_visible {
                    self.mode = MonsterAIMode::Hunting;
                }
            }
            MonsterAIMode::Hunting => {
                if is_player_visible {
                    self.last_known_player_position = Some(player_point);
                }
                if !is_player_visible && Some(monster_pos) == self.last_known_player_position {
                    self.mode = MonsterAIMode::Wandering;
                    self.last_known_player_position = None;
                }
            }
            MonsterAIMode::Wandering => {
                if is_player_visible {
                    self.mode = MonsterAIMode::Hunting;
                }
            }
        }

        // --- STEP 3: PATHFINDING AND INTENT (Isolated Map access) ---
        // We use an inner scope to ensure the Map resource borrow is dropped
        // before we call world.write_message() later.
        let intent_to_send = {
            let map = world.resource::<Map>();

            match self.mode {
                MonsterAIMode::Hunting => {
                    if let Some(target) = self.last_known_player_position {
                        let path = a_star_search(
                            map.point2d_to_index(monster_pos),
                            map.point2d_to_index(target),
                            map,
                        );

                        if path.success && path.steps.len() > 1 {
                            let next_step = map.index_to_point2d(path.steps[1]);
                            let dir = Direction::from_pos(
                                &Position::from_point(monster_pos),
                                &Position::from_point(next_step),
                            );
                            Some(MovementIntent { entity, dir })
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                MonsterAIMode::Wandering => {
                    let mut directions = Direction::ALL.to_vec();
                    directions.shuffle(&mut rng);

                    let mut chosen_dir = None;
                    for dir in directions {
                        let target = monster_pos + dir.offset();
                        if map.in_bounds(target)
                            && is_walkable(map.tiles[map.xy_idx(target.x, target.y)])
                        {
                            chosen_dir = Some(dir);
                            break;
                        }
                    }
                    chosen_dir.map(|dir| MovementIntent { entity, dir })
                }
                _ => None,
            }
        };

        // --- STEP 4: FINAL MUTATION ---
        // All previous borrows (Query, Map, Position) are guaranteed dead here.
        if let Some(intent) = intent_to_send {
            world.write_message(intent);
        } else {
            world.write_message(WaitIntent { entity });
        }
    }
}
