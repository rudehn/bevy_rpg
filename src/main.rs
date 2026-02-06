use bevy::prelude::*;

mod assets_plugin;
mod components;
mod constants;
mod map;
mod player;
mod states;

use crate::states::StatePlugin;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(ImagePlugin::default_nearest()),
            StatePlugin,
        ))
        .run();
}

// use bevy::prelude::*;
// use bevy_light_2d::prelude::*;

// fn main() {
//     App::new()
//         .add_plugins((DefaultPlugins, Light2dPlugin))
//         .add_systems(Startup, setup)
//         .add_systems(Update, update_light_intensity) // Add the system to update the light
//         .run();
// }

// // Setup function (from the bevy_light_2d example)
// fn setup(mut commands: Commands) {
//     commands.spawn((Camera2d, Light2d::default()));
//     // Spawn a PointLight2d with an initial intensity
//     commands.spawn(PointLight2d {
//         intensity: 3.0,
//         radius: 100.0,
//         ..default()
//     });
// }

// // System to update the light intensity dynamically
// fn update_light_intensity(
//     // Query for entities that have the PointLight2d component
//     mut query: Query<&mut PointLight2d>,
//     time: Res<Time>,
// ) {
//     for mut light in &mut query {
//         // Update the intensity using a sine wave for a pulsating effect
//         // The intensity value can be any f32
//         light.intensity = (time.elapsed_secs().sin() * 2.0 + 3.0).max(0.0);
//     }
// }
