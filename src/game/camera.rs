use bevy::prelude::*;
use bevy_light_2d::light::{AmbientLight2d, Light2d};

use crate::{components::Position, player::Player};

// Components for distinguishing cameras
#[derive(Component)]
pub struct MinimapCamera;

#[derive(Component)]
pub struct MainCamera;

// Camera setup functions
pub fn setup_camera(mut commands: Commands) {
    let mut projection = OrthographicProjection::default_2d();
    projection.scale = 0.25;
    commands.spawn((
        Camera2d,
        Projection::Orthographic(projection),
        Light2d {
            ambient_light: AmbientLight2d {
                brightness: 0.1,
                ..default()
            },
        },
        MainCamera,
    ));
}

// pub fn setup_minimap_camera(mut commands: Commands) {
//     commands.spawn((
//         Camera2d,
//         Camera {
//             order: 1,
//             clear_color: ClearColorConfig::Default,
//             viewport: Some(Viewport {
//                 physical_position: UVec2::new(10, 10),
//                 physical_size: UVec2::new(200, 200),
//                 ..default()
//             }),
//             ..default()
//         },
//         Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::splat(0.75)),
//         Light2d::default(),
//         MinimapCamera,
//     ));
// }

// Camera movement systems
pub fn move_camera(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    player_query: Query<&Transform, (With<Player>, Changed<Position>)>,
    mut camera_query: Query<(&mut Transform, &mut Projection), (With<MainCamera>, Without<Player>)>,
) {
    for ((mut camera_transform, mut camera_projection)) in camera_query.iter_mut() {
        if let Ok(player_transform) = player_query.single() {
            camera_transform.translation.x = player_transform.translation.x;
            camera_transform.translation.y = player_transform.translation.y;
        }

        // Scale camera zoom
        let Projection::Orthographic(ortho) = &mut *camera_projection else {
            return;
        };

        if keyboard_input.pressed(KeyCode::KeyZ) {
            ortho.scale += 0.1;
        }

        if keyboard_input.pressed(KeyCode::KeyX) {
            ortho.scale -= 0.1;
        }

        ortho.scale = ortho.scale.clamp(0.25, 1.0);

        let z = camera_transform.translation.z;
        camera_transform.translation.z = z;
    }
}

// pub fn move_minimap_camera(
//     player_query: Query<&Transform, With<Player>>,
//     mut minimap_camera_query: Query<&mut Transform, With<MinimapCamera>>,
// ) {
//     if let Ok(player_transform) = player_query.single() {
//         if let Ok(mut minimap_camera_transform) = minimap_camera_query.single_mut() {
//             minimap_camera_transform.translation.x = player_transform.translation.x;
//             minimap_camera_transform.translation.y = player_transform.translation.y;
//         }
//     }
// }
