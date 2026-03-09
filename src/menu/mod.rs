use crate::game::AppState;
use bevy::prelude::*;
use bevy::ui::{
    AlignItems, BackgroundColor, BorderColor, Interaction, JustifyContent, UiRect, Val,
};

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<OnMainMenuScreen>()
            .register_type::<OnGameOverScreen>()
            .register_type::<OnVictoryScreen>()
            // Essential UI components for reflection
            .register_type::<ChildOf>()
            .register_type::<Node>()
            .register_type::<Visibility>()
            .register_type::<BackgroundColor>()
            .register_type::<BorderColor>()
            .register_type::<Button>()
            .register_type::<Text>()
            .register_type::<TextFont>()
            .register_type::<TextColor>()
            .register_type::<Interaction>()
            .add_systems(Startup, debug_type_registration)
            .add_systems(OnEnter(AppState::Menu), menu_setup)
            .add_systems(
                Update,
                (menu_action, dump_menu_scene).run_if(in_state(AppState::Menu)),
            )
            .add_systems(OnExit(AppState::Menu), despawn_screen::<OnMainMenuScreen>)
            .add_systems(OnEnter(AppState::GameOver), game_over_setup)
            .add_systems(
                Update,
                game_over_action.run_if(in_state(AppState::GameOver)),
            )
            .add_systems(
                OnExit(AppState::GameOver),
                despawn_screen::<OnGameOverScreen>,
            )
            .add_systems(OnEnter(AppState::Victory), victory_setup)
            .add_systems(
                Update,
                victory_action.run_if(in_state(AppState::Victory)),
            )
            .add_systems(
                OnExit(AppState::Victory),
                despawn_screen::<OnVictoryScreen>,
            );
    }
}

fn dump_menu_scene(world: &mut World) {
    if !world
        .resource::<ButtonInput<KeyCode>>()
        .just_pressed(KeyCode::KeyS)
    {
        return;
    }

    info!("Dumping menu scene...");

    let mut entities_to_extract = Vec::new();
    let mut query = world.query_filtered::<Entity, Or<(With<Node>, With<OnMainMenuScreen>)>>();
    for entity in query.iter(world) {
        entities_to_extract.push(entity);
    }

    let mut builder = DynamicSceneBuilder::from_world(world);
    // builder = builder
    //     .deny_component::<ComputedNode>()
    //     .deny_component::<ContentSize>()
    //     .deny_component::<TextLayoutInfo>()
    //     .deny_component::<TextNodeFlags>()
    //     .deny_component::<InheritedVisibility>()
    //     .deny_component::<ViewVisibility>()
    //     .deny_component::<GlobalTransform>()
    //     .deny_component::<UiGlobalTransform>()
    //     // Optional: Deny interaction states so the menu loads in a "neutral" state
    //     .deny_component::<Interaction>();
    for entity in entities_to_extract {
        builder = builder.extract_entity(entity);
    }

    let scene = builder.build();
    let type_registry = world.resource::<AppTypeRegistry>().clone();
    let registry = type_registry.read();

    match scene.serialize(&registry) {
        Ok(serialized) => {
            let _ = std::fs::create_dir_all("assets/scenes");
            if let Ok(_) = std::fs::write("assets/scenes/dumped_menu.scn.ron", serialized) {
                info!("Successfully dumped menu to assets/scenes/dumped_menu.scn.ron");
            }
        }
        Err(e) => {
            error!("Failed to serialize menu scene: {:?}", e);
        }
    }
}

fn debug_type_registration(type_registry: Res<AppTypeRegistry>) {
    let registry = type_registry.read();
    let types_to_check = vec![
        ("Node", std::any::type_name::<Node>()),
        ("BackgroundColor", std::any::type_name::<BackgroundColor>()),
        ("BorderColor", std::any::type_name::<BorderColor>()),
        ("Button", std::any::type_name::<Button>()),
        ("Text", std::any::type_name::<Text>()),
        ("TextFont", std::any::type_name::<TextFont>()),
        ("TextColor", std::any::type_name::<TextColor>()),
        ("ChildOf", std::any::type_name::<ChildOf>()),
        ("Visibility", std::any::type_name::<Visibility>()),
        (
            "OnMainMenuScreen",
            std::any::type_name::<OnMainMenuScreen>(),
        ),
    ];

    info!("--- COMPONENT REFLECTION PATHS ---");
    for (label, type_name) in types_to_check {
        if let Some(registration) = registry.get_with_type_path(type_name) {
            info!("{}: {}", label, registration.type_info().type_path());
        } else {
            warn!("{}: NOT REGISTERED ({})", label, type_name);
        }
    }
}

// Tag component to mark entities added by the setup_menu system.
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct OnMainMenuScreen;

// Tag component to mark entities added by the game_over_setup system.
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct OnGameOverScreen;

// Tag component to mark entities added by the victory_setup system.
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct OnVictoryScreen;

fn menu_setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::NONE),
            OnMainMenuScreen,
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Button,
                    Node {
                        width: Val::Px(150.0),
                        height: Val::Px(65.0),
                        border: UiRect::all(Val::Px(5.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.15, 0.15, 0.15)),
                    BorderColor::all(Color::BLACK),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Text::new("Play"),
                        TextFont {
                            font: asset_server.load("fonts/Macondo-Regular.ttf"),
                            font_size: 40.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });
        });
}

fn menu_action(
    interaction_query: Query<&Interaction, (With<Button>, Changed<Interaction>)>,
    mut next_state: ResMut<NextState<AppState>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
) {
    for interaction in &interaction_query {
        if *interaction == Interaction::Pressed {
            next_state.set(AppState::InGame);
            return;
        }
    }

    if keyboard_input.just_pressed(KeyCode::Enter) {
        next_state.set(AppState::InGame);
    }
}

fn game_over_setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
            OnGameOverScreen,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("YOU DIED!"),
                TextFont {
                    font: asset_server.load("fonts/Macondo-Regular.ttf"),
                    font_size: 60.0,
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.0, 0.0)),
            ));

            parent
                .spawn((
                    Button,
                    Node {
                        width: Val::Px(250.0),
                        height: Val::Px(65.0),
                        border: UiRect::all(Val::Px(5.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        margin: UiRect::top(Val::Px(20.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.15, 0.15, 0.15)),
                    BorderColor::all(Color::WHITE),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Text::new("Return to Menu"),
                        TextFont::from_font_size(30.0),
                        TextColor(Color::WHITE),
                    ));
                });
        });
}

fn game_over_action(
    interaction_query: Query<&Interaction, (With<Button>, Changed<Interaction>)>,
    mut next_state: ResMut<NextState<AppState>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
) {
    for interaction in &interaction_query {
        if *interaction == Interaction::Pressed {
            next_state.set(AppState::Menu);
            return;
        }
    }

    if keyboard_input.just_pressed(KeyCode::Enter) {
        next_state.set(AppState::Menu);
    }
}

fn victory_setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
            OnVictoryScreen,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("VICTORY!"),
                TextFont {
                    font: asset_server.load("fonts/Macondo-Regular.ttf"),
                    font_size: 60.0,
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.84, 0.0)), // Gold
            ));

            parent.spawn((
                Text::new("You have retrieved the Amulet of Bevy!"),
                TextFont::from_font_size(30.0),
                TextColor(Color::WHITE),
            ));

            parent
                .spawn((
                    Button,
                    Node {
                        width: Val::Px(250.0),
                        height: Val::Px(65.0),
                        border: UiRect::all(Val::Px(5.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        margin: UiRect::top(Val::Px(40.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.15, 0.15, 0.15)),
                    BorderColor::all(Color::WHITE),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Text::new("Return to Menu"),
                        TextFont::from_font_size(30.0),
                        TextColor(Color::WHITE),
                    ));
                });
        });
}

fn victory_action(
    interaction_query: Query<&Interaction, (With<Button>, Changed<Interaction>)>,
    mut next_state: ResMut<NextState<AppState>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
) {
    for interaction in &interaction_query {
        if *interaction == Interaction::Pressed {
            next_state.set(AppState::Menu);
            return;
        }
    }

    if keyboard_input.just_pressed(KeyCode::Enter) {
        next_state.set(AppState::Menu);
    }
}

fn despawn_screen<T: Component>(to_despawn: Query<Entity, With<T>>, mut commands: Commands) {
    for entity in &to_despawn {
        commands.entity(entity).despawn();
    }
}
