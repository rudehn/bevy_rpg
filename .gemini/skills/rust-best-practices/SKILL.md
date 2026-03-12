---
name: rust-best-practices
description: Enforce Rust coding best practices and Bevy-specific conventions. Use this skill BEFORE modifying any `.rs` file to ensure changes are idiomatic, efficient, and follow the project's architectural mandates (e.g., ECS first, proper ownership, error handling).
---

# Rust Best Practices

This skill provides a checklist and set of guidelines for modifying Rust code in the `bevy_rpg` project.

## Workflow

1.  **Analyze the Target File**: Before proposing any change to a `.rs` file, identify the current patterns (e.g., how systems are structured, how components are queried).
2.  **Verify Against Guidelines**: Ensure the proposed change follows:
    - [Bevy Conventions](references/bevy_conventions.md)
    - [General Rust Lints](references/rust_lints.md)
3.  **Proactive Review**: If a proposed change violates a best practice (e.g., using `unwrap()`, unnecessary `clone()`, or non-idiomatic ECS usage), adjust the implementation BEFORE applying it.

## Key Checkpoints

- **ECS First**: Use Bevy's `Query`, `Res`, `ResMut`, and `Commands` correctly.
- **Naming**: `snake_case` for functions/variables, `PascalCase` for types/structs.
- **Ownership**: Minimize `.clone()`. Use references where appropriate.
- **Error Handling**: Prefer `Result` and `Option` over `panic!` or `unwrap()`.
- **Modularity**: Keep systems decoupled and focused on a single task.
