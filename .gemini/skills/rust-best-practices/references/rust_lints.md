# General Rust Lints and Best Practices

## Naming

- **Functions/Variables**: `snake_case`.
- **Types/Traits/Structs/Enums**: `PascalCase`.
- **Macros**: `snake_case!`.
- **Constants/Statics**: `SCREAMING_SNAKE_CASE`.

## Ownership and Borrowing

- **Minimize Clones**: Avoid `.clone()` unless necessary for ownership. Pass references (`&T`) instead.
- **Copy vs Clone**: Prefer `Copy` for small, fixed-size types (e.g., `Position`).
- **Implicit Borrowing**: Prefer `&str` over `&String` in function arguments.

## Error Handling

- **Result and Option**: Use them extensively. Use `?` for error propagation.
- **Avoid Panic**: Never use `unwrap()` or `expect()` in production code. Prefer `map()`, `and_then()`, or `unwrap_or_else()`.
- **Custom Errors**: Define domain-specific errors for complex operations.

## Comments and Documentation

- **Doc Comments**: Use `///` for documenting public API, structs, and functions. Comments should provide a summary sentance in 15 words or less, a blank line and then a detailed description if applicable.
- **In-code Comments**: Use sparingly. Code should be self-documenting through clear naming. Focus more on describing complicated logic.
