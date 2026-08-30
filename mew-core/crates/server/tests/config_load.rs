use mewcode_protocol::env::OPENROUTER_API_KEY;
use mewcode_server::ServerConfig;

const PREFIXED_OPENROUTER_API_KEY: &str = "MEWCODE_OPENROUTER_API_KEY";

#[test]
fn openrouter_environment_precedence_is_prefixed_then_canonical() {
    let canonical = std::env::var(OPENROUTER_API_KEY).ok();
    let prefixed = std::env::var(PREFIXED_OPENROUTER_API_KEY).ok();
    let _guard = EnvGuard {
        canonical,
        prefixed,
    };

    remove(PREFIXED_OPENROUTER_API_KEY);
    set(OPENROUTER_API_KEY, "canonical");
    assert_eq!(
        ServerConfig::load().unwrap().openrouter_api_key.as_deref(),
        Some("canonical")
    );

    set(PREFIXED_OPENROUTER_API_KEY, "prefixed");
    assert_eq!(
        ServerConfig::load().unwrap().openrouter_api_key.as_deref(),
        Some("prefixed")
    );
}

struct EnvGuard {
    canonical: Option<String>,
    prefixed: Option<String>,
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        restore(OPENROUTER_API_KEY, self.canonical.take());
        restore(PREFIXED_OPENROUTER_API_KEY, self.prefixed.take());
    }
}

fn set(key: &str, value: &str) {
    unsafe { std::env::set_var(key, value) };
}

fn remove(key: &str) {
    unsafe { std::env::remove_var(key) };
}

fn restore(key: &str, value: Option<String>) {
    match value {
        Some(value) => set(key, &value),
        None => remove(key),
    }
}
