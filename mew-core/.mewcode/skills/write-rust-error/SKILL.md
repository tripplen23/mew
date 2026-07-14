---
name: write-rust-error
description: Diagnose a Rust compiler error and propose a fix. Use when the user pastes a Rust compiler error and asks for help.
---

# How to fix a Rust compiler error

When invoked via `use_skill("write-rust-error")`:

1. **Read the error carefully.** Rust's errors are dense but precise.
   Note the error code (`E0xxx`), the span (line and column), and the
   "help" notes the compiler prints.

2. **Look at the surrounding code.** Use `read_file` to read the
   referenced file. Do not guess — the actual source is the source of
   truth.

3. **Categorise the error.** The common categories are:
   - **Borrow checker** (`E0502`, `E0503`, `E0505`, `E0506`, `E0507`,
     `E0382`, `E0384`): ownership or lifetime problem.
   - **Type mismatch** (`E0308`, `E0277`): wrong type, often a missing
     trait bound or an `Option`/`Result` not unwrapped.
   - **Missing import** (`E0432`, `E0433`): use `use` or fix the path.
   - **Async** (`E0733`, `E0769`): `async` used in a non-async context,
     or missing `.await`.
   - **Trait** (`E0277`, `E0599`): method doesn't exist on the type,
     often a missing trait import.

4. **Propose a fix.** Show the *smallest* change that resolves the
   error. Explain *why* it works in one or two sentences. If the fix
   involves a design choice (e.g. `clone()` vs `&` vs `Cow`), surface
   that choice rather than picking silently.

5. **Pitfalls to flag:**
   - Adding `#[allow(...)]` instead of fixing the root cause.
   - Using `.unwrap()` to silence a Result.
   - `String` ↔ `&str` conversions in a hot loop.
   - Cloning a large buffer when a reference would do.

## What NOT to do

- Don't suggest adding `unsafe` unless the user asks.
- Don't propose re-designs of the surrounding code; stay scoped to the
  error.
- Don't use `cargo expand` or other tools — you have `read_file`, use
  it.
