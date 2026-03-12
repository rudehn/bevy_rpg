# Bevy 0.17 Conventions

## ECS (Entity Component System)

- **System Structure**: Focus systems on a single task. Use descriptive names (e.g., `sync_entity_transforms`, `fov_update_system`).
- **Query Management**: Use `Query<&Component>` for read access and `Query<&mut Component>` for write access. Avoid `Query<Entity, &mut Transform>` if `Entity` is not used.
- **Resource Usage**: Use `Res<T>` and `ResMut<T>` correctly. Ensure resources are initialized via `App::init_resource` or `App::insert_resource`.
- **Commands**: Use `Commands` to spawn/despawn entities or add/remove components asynchronously.
- **Entity Identification**: Use `Entity` IDs for references between entities, not raw pointers or indices.

## Components and Resources

- **Structs**: Use `#[derive(Component)]` for ECS data.
- **Resources**: Use `#[derive(Resource)]` for global state.
- **Markers**: Use empty structs like `struct Player;` as marker components.

## Performance

- **Change Detection**: Use `Changed<T>` or `Added<T>` filters in queries to avoid unnecessary computation.
- **Parallelism**: Bevy schedules systems in parallel by default. Avoid `ResMut` on global resources and `&mut World` in common systems if not needed to reduce synchronization overhead.
