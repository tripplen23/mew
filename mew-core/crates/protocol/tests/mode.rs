//! Integration tests for `mewcode_protocol::mode`.

use mewcode_protocol::Mode;

#[test]
fn roundtrip() {
    for m in [Mode::Build, Mode::Plan] {
        let s = m.as_str();
        let parsed: Mode = s.parse().unwrap();
        assert_eq!(m, parsed);
    }
}

#[test]
fn invalid() {
    assert!("oops".parse::<Mode>().is_err());
}

#[test]
fn default_is_build() {
    assert_eq!(Mode::default(), Mode::Build);
}
