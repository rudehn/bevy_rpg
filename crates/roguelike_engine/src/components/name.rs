//! Display name component.

use bevy::ecs::component::Component;

/// A human-readable display name attached to an entity.
///
/// Used by UI, log messages, and tooltips. The engine treats this as
/// an opaque string; games choose naming conventions (capitalization,
/// articles, i18n key) as they see fit.
#[derive(Component)]
pub struct Name(pub String);
