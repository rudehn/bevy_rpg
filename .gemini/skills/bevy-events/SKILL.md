---
name: bevy-events
description: Strict message-based event handling for Bevy 0.17+. Use when creating, reading, or sending events/messages to ensure the MessageReader/MessageWriter paradigm is followed.
---

# Event Handling Rules
This project uses a strict `MessageReader` and `MessageWriter` paradigm for all system communication.

## Implementation Guidelines
* **Use `MessageReader<T>`** to read messages in a system.
* **Use `MessageWriter<T>`** to send messages from a system.
* **Terminology**: Refer to events exclusively as "Messages" in architectural plans.

## Restrictions
* **NEVER** use Bevy's default `EventReader<T>` or `EventWriter<T>`.
* **NEVER** suggest migrating to standard Bevy events.

## Example
```rust
fn handle_damage_messages(
    mut messages: MessageReader<DamageMessage>,
    mut health_query: Query<&mut Health>,
) {
    for message in messages.read() {
        // ... logic
    }
}
fn emit_damage_messages(
    mut writer: MessageWriter<DamageMessage>,
) {
    writer.write(DamageMessage { amount: 10 });
}
```
