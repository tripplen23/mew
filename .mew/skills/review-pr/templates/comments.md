# PR Review Comment Templates

Reusable comment blocks for the most common review findings. Copy
the appropriate block into the PR review, edit the `<…>` placeholders,
and you're done. Read with
`skill_view(name="review-pr", path="templates/comments.md")`.

---

## "Add a test"

> Nice change. Could you add a test that exercises the failing
> input as well? Today's test only covers the happy path.
>
> ```rust
> #[test]
> fn <name>_rejects_<input>() {
>     let result = <call_under_test>(<bad_input>);
>     assert!(matches!(result, Err(<error>)));
> }
> ```

---

## "Concurrent access"

> This shared state isn't behind a `Mutex` / `RwLock`, so two
> concurrent calls to `<method>` could see a torn read. Could you
> either wrap it in a lock or move it to thread-local / per-task
> storage?

---

## "Error type"

> The current `Result<T, String>` is too generic — callers can't
> pattern-match on the cause. Could you introduce a `<Error>` enum
> (thiserror is fine) and convert at the boundary with
> `.map_err(...)`?

---

## "Naming"

> `<name>` is a bit vague. From the call site (`<where>`) it's
> clear you mean `<specific>`. Could you rename to `<suggested>`?

---

## "Docs"

> This is a public item but it has no doc comment. Even a one-liner
> explaining the contract (not just the *what*) would help — what
> does the caller need to know that isn't obvious from the type
> signature?
