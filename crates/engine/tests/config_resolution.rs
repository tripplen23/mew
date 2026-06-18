// Feature: session-flow-and-engine-v0 — example tests for engine config resolution.

use mewcode_engine::EngineConfig;
use mewcode_engine::config::{DEFAULT_BASE_URL, ENV_BASE_URL};
use mewcode_protocol::env::OPENCODE_GO_API_KEY;

// Env vars are process-global, so all cases live in ONE serial test
// (cargo runs tests in a binary on parallel threads). The trade-off is no
// per-case isolation; upgrade path is the `serial_test` crate if these ever
// need to split. We snapshot and restore the two vars we touch.
#[test]
fn config_resolution_uses_defaults_key_and_override() {
    let prior_key = std::env::var(OPENCODE_GO_API_KEY).ok();
    let prior_base = std::env::var(ENV_BASE_URL).ok();
    let _guard = EnvGuard {
        key: prior_key,
        base: prior_base,
    };

    // With only OPENCODE_GO_API_KEY set and no base override, the
    // key is taken from that variable and the base URL falls back to default.
    set(OPENCODE_GO_API_KEY, "sk-test-123");
    remove(ENV_BASE_URL);
    let cfg = EngineConfig::from_env().expect("key present => Ok");
    assert_eq!(
        cfg.api_key, "sk-test-123",
        "key is read from OPENCODE_GO_API_KEY"
    );
    assert_eq!(DEFAULT_BASE_URL, "https://opencode.ai/zen/go");
    assert_eq!(
        cfg.base_url, DEFAULT_BASE_URL,
        "base URL defaults to OpenCode Go production"
    );

    // The key is read SOLELY from OPENCODE_GO_API_KEY — once unset, no
    // other variable can supply it, so resolution fails.
    remove(OPENCODE_GO_API_KEY);
    assert!(
        EngineConfig::from_env().is_err(),
        "no fallback source supplies the key"
    );

    // A non-empty MEWCODE_ENGINE_BASE_URL overrides the default.
    set(OPENCODE_GO_API_KEY, "sk-test-123");
    set(ENV_BASE_URL, "https://example.test/base");
    let cfg = EngineConfig::from_env().expect("key present => Ok");
    assert_eq!(
        cfg.base_url, "https://example.test/base",
        "non-empty override replaces the default base URL"
    );
}

struct EnvGuard {
    key: Option<String>,
    base: Option<String>,
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        restore(OPENCODE_GO_API_KEY, self.key.take());
        restore(ENV_BASE_URL, self.base.take());
    }
}

fn set(key: &str, value: &str) {
    // SAFETY: single-threaded serial test; no other thread reads the env here.
    unsafe { std::env::set_var(key, value) };
}

fn remove(key: &str) {
    // SAFETY: single-threaded serial test; no other thread reads the env here.
    unsafe { std::env::remove_var(key) };
}

fn restore(key: &str, prior: Option<String>) {
    match prior {
        Some(v) => set(key, &v),
        None => remove(key),
    }
}
