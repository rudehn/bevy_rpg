use bevy::camera::visibility::RenderLayers;
use bevy::{camera::Viewport, prelude::*};

use crate::{game::AppState, player::Player};

// Components for distinguishing cameras
#[allow(dead_code)]
#[derive(Component)]
pub struct MinimapCamera;

#[derive(Component)]
pub struct MainCamera;

#[derive(Component)]
pub struct UiCamera;

// Camera setup functions
pub fn setup_camera(mut commands: Commands, windows: Query<&Window>) {
    // 1. UI Camera
    // Order 1 means it renders AFTER (on top of) the game camera.
    commands.spawn((
        Camera2d,
        Camera {
            order: 1,
            // By default (in Menu), it should clear the background.
            // When InGame, we will set this to None to see the game.
            ..default()
        },
        IsDefaultUiCamera,
        UiCamera,
    ));

    // 2. Main Game Camera
    let mut projection = OrthographicProjection::default_2d();
    projection.scale = 0.25;

    let Ok(window) = windows.single() else {
        // Fallback if window not yet available
        commands.spawn((
            Camera2d,
            Camera {
                order: 0,
                // The game camera will clear the background when active.
                is_active: false,
                ..default()
            },
            Projection::Orthographic(projection),
            MainCamera,
            RenderLayers::layer(1),
        ));
        return;
    };

    // Viewport restrictions:
    // Bottom: 150px for Game Log
    // Right: 200px for Player Stats
    let log_height = 150.0;
    let stats_width = 200.0;

    let viewport_height = (window.height() - log_height).max(1.0);
    let viewport_width = (window.width() - stats_width).max(1.0);

    commands.spawn((
        Camera2d,
        Camera {
            order: 0,
            viewport: Some(Viewport {
                physical_position: UVec2::new(0, 0),
                physical_size: UVec2::new(
                    (viewport_width * window.resolution.scale_factor()) as u32,
                    (viewport_height * window.resolution.scale_factor()) as u32,
                ),
                ..default()
            }),
            is_active: false,
            ..default()
        },
        Projection::Orthographic(projection),
        MainCamera,
        RenderLayers::layer(1),
    ));
}

/// System that toggles the MainCamera active status and UiCamera clear behavior based on game state.
pub fn toggle_main_camera_visibility(
    state: Res<State<AppState>>,
    mut q_main_camera: Query<&mut Camera, (With<MainCamera>, Without<UiCamera>)>,
    mut q_ui_camera: Query<&mut Camera, (With<UiCamera>, Without<MainCamera>)>,
) {
    let Ok(mut main_cam) = q_main_camera.single_mut() else {
        return;
    };
    let Ok(mut ui_cam) = q_ui_camera.single_mut() else {
        return;
    };

    if *state.get() == AppState::InGame {
        main_cam.is_active = true;
        ui_cam.clear_color = ClearColorConfig::None;
    } else {
        main_cam.is_active = false;
        ui_cam.clear_color = ClearColorConfig::Default;
    }
}

// Camera movement systems
pub fn move_camera(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    player_query: Query<&Transform, With<Player>>,
    mut camera_query: Query<(&mut Transform, &mut Projection), (With<MainCamera>, Without<Player>)>,
) {
    for (mut camera_transform, mut camera_projection) in camera_query.iter_mut() {
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
