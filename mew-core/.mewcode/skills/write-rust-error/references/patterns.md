# Rust Error Writing Patterns

A condensed reference for the write-rust-error skill. Read with
`skill_view(name="write-rust-error", path="references/patterns.md")`.

## `thiserror` for library code

```rust
#[derive(Debug, thiserror::Error)]
pub enum MyError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("io error: {source}")]
    Io {
        #[source]
        source: std::io::Error,
    },
    #[error("not found: {path}")]
    NotFound { path: std::path::PathBuf },
}
```

## `anyhow` for application / bin code

```rust
fn main() -> anyhow::Result<()> {
    let config = std::fs::read_to_string("config.toml")?;
    // ...
    Ok(())
}
```

## Anti-patterns to flag

- `panic!` / `.unwrap()` / `.expect()` in library code on user input
- `Result<T, String>` instead of a proper error enum
- Errors that include `{:?}` formatting (leak internals)
- `match err { _ => eprintln!(...) }` (silent error swallow)
- Error messages without actionable context (e.g. "failed" with no
  what / where / why)

## Good message checklist

- [ ] What went wrong (the action that failed)
- [ ] Where it went wrong (file, function, or input value)
- [ ] Why it might have gone wrong (the cause, if known)
- [ ] What the caller can do (the actionable hint)
