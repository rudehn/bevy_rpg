In a traditional roguelike, status effects (buffs and debuffs) are what transform a simple "stat-check" into a tactical encounter. Because you are using Bevy, the most efficient way to handle this is to treat status effects as **individual components** or **data-driven entities**.

Here is a design for a robust, modular status effect system.

---

## 1. The Component-Per-Effect Model

Rather than a single "Status" enum, use specific components for each effect. This allows you to use Bevy’s query system to only target entities that are, for example, currently `Poisoned`.

### Common Status Components

* **`Poisoned { damage: i32, duration: i32 }`**: Deals damage every turn.
* **`Stunned { duration: i32 }`**: Skips the entity's turn.
* **`Bleeding { damage_per_move: i32 }`**: Deals damage only when the entity moves.
* **`StrengthBuff { amount: i32, duration: i32 }`**: Increases `damage_bonus`.

---

## 2. The Lifecycle: Apply, Tick, Expire

To maintain the clean pipeline you’ve already built, status effects should follow a predictable lifecycle triggered by your `TurnEndEvent`.

### Step A: The Application Message

Don't just insert components directly. Use a message so other systems (like the UI or Log) can react.

```rust
#[derive(Message)]
pub struct ApplyStatusMessage {
    pub target: Entity,
    pub status: StatusType, // An enum used for the message only
    pub duration: i32,
}

```

### Step B: The Tick System

This system runs whenever a turn ends. It decrements durations and applies "over-time" logic.

```rust
fn status_tick_system(
    mut commands: Commands,
    mut turn_end_reader: MessageReader<TurnEndEvent>,
    mut poison_query: Query<(Entity, &mut Health, &mut Poisoned, &Name)>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for _ in turn_end_reader.read() {
        for (entity, mut health, mut poison, name) in poison_query.iter_mut() {
            // Apply effect
            health.current -= poison.damage;
            log_writer.write(GameLogMessage(format!("{} takes {} poison damage.", name.0, poison.damage)));

            // Handle duration
            poison.duration -= 1;
            if poison.duration <= 0 {
                commands.entity(entity).remove::<Poisoned>();
                log_writer.write(GameLogMessage(format!("{} is no longer poisoned.", name.0)));
            }
        }
    }
}

```

---

## 3. Hooking into the Combat Engine

Status effects should "intercept" your combat messages to modify outcomes.

### The "Strength" Hook

You can create a system that runs **between** `damage_roll_system` and `armor_reduction_system`. It reads the `DamageReductionMessage` and modifies the `raw_damage` if the attacker has a `StrengthBuff`.

```rust
fn strength_buff_modifier_system(
    mut messages: MessageReader<DamageReductionMessage>,
    mut writer: MessageWriter<DamageReductionMessage>, // Pass it along modified
    query: Query<&StrengthBuff>,
) {
    for mut msg in messages.read() {
        if let Ok(buff) = query.get(msg.attacker) {
            msg.raw_damage += buff.amount;
        }
        writer.write(msg); 
    }
}

```

*Note: This requires careful ordering in your `SystemSet` to ensure the "Modifier" systems run in the correct sequence.*

---

## 4. Advanced System Ideas

* **Effect Stacking:** If a player is hit with Poison while already poisoned, does it refresh the duration, or does the damage per turn increase? You can handle this in the "Application" system by checking `Has<Poisoned>`.
* **Status Immunities:** Some monsters (like Undead) should have a `PoisonImmunity` component. The `ApplyStatusMessage` handler would check for this component before adding the `Poisoned` component.
* **Synergies:** Create "Reaction" status effects. If a target is `Chilled` and then hit with `Water` damage, they become `Frozen` (Stunned).

---

## 5. Tactical Implementation: The "Aura" System

In roguelikes, positioning is key. You can use status effects to create **Auras**.

* An entity has a `FireAura` component.
* A system runs every turn, finds all entities within a 1-tile radius of the aura-bearer, and sends an `ApplyStatusMessage` for a "Singed" effect or direct fire damage.
