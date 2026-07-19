use mewcode_engine::harness::user_text_with_file_context;
use mewcode_protocol::{Message, MessagePart};

#[test]
fn user_text_with_file_context_reads_at_mentions() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("a.rs"), "fn main() {}\n").unwrap();
    let msg = Message::user(vec![MessagePart::Text {
        text: "explain @a.rs".to_string(),
    }]);

    let out = user_text_with_file_context(&[msg], root.path()).unwrap();

    assert!(out.contains("@a.rs"));
    assert!(out.contains("fn main() {}"));
    assert!(out.contains("User message:\nexplain @a.rs"));
}
