//! Integration tests for `mewcode_protocol::model`.

use std::collections::HashSet;

use mewcode_protocol::ModelId;

#[test]
fn all_have_unique_strs() {
    let mut seen = HashSet::new();
    for m in ModelId::ALL {
        assert!(seen.insert(m.as_str()), "duplicate: {:?}", m);
    }
}

#[test]
fn default_is_minimax_m3() {
    assert_eq!(ModelId::default(), ModelId::MiniMaxM3);
}

#[test]
fn parse_known() {
    assert_eq!("minimax-m3".parse::<ModelId>().unwrap(), ModelId::MiniMaxM3);
    assert_eq!("MiniMax M3".parse::<ModelId>().unwrap(), ModelId::MiniMaxM3);
}

#[test]
fn parse_unknown() {
    assert!("gpt-99".parse::<ModelId>().is_err());
}

#[test]
fn serde_roundtrip() {
    for m in ModelId::ALL {
        let s = serde_json::to_string(m).unwrap();
        let back: ModelId = serde_json::from_str(&s).unwrap();
        assert_eq!(m, &back, "serde roundtrip failed for {m:?}");
    }
}
