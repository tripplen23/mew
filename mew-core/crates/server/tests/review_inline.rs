//! Phase A: parsing the skill's machine-parseable findings and anchoring
//! them to diff hunks. Pure functions — no network.

use mewcode_server::services::github_bot::{
    InlineComment, anchor_inline_comments, diff_new_lines, parse_findings,
};

#[test]
fn parse_findings_extracts_path_line_body() {
    let text = "\
## src/foo.rs
- 42: 🔴 Must fix: crashes when x is None
- 17: 🟢 Suggestion: rename y
## src/bar.rs
- 3: 🟡 Should fix: unhandled error
Verdict: Needs work
Must-fix: 42
";
    let findings = parse_findings(text);
    assert_eq!(
        findings,
        vec![
            InlineComment {
                path: "src/foo.rs".into(),
                line: 42,
                body: "🔴 Must fix: crashes when x is None".into()
            },
            InlineComment {
                path: "src/foo.rs".into(),
                line: 17,
                body: "🟢 Suggestion: rename y".into()
            },
            InlineComment {
                path: "src/bar.rs".into(),
                line: 3,
                body: "🟡 Should fix: unhandled error".into()
            },
        ]
    );
}

#[test]
fn parse_findings_skips_unparseable_lines() {
    let text = "\
## src/foo.rs
- 42: 🔴 Must fix: ok
- not-a-line: 🔴 Must fix: skipped
- 7:   trailing space stays
free-floating text
## src/bar.rs
- x: skipped
";
    let findings = parse_findings(text);
    assert_eq!(
        findings,
        vec![
            InlineComment {
                path: "src/foo.rs".into(),
                line: 42,
                body: "🔴 Must fix: ok".into()
            },
            InlineComment {
                path: "src/foo.rs".into(),
                line: 7,
                body: "trailing space stays".into()
            },
        ]
    );
}

#[test]
fn parse_findings_ignores_lines_before_first_header() {
    let text = "preamble\n- 1: 🟡 Should fix: orphan\n";
    assert!(parse_findings(text).is_empty());
}

#[test]
fn diff_new_lines_maps_hunk_ranges() {
    let diff = "\
diff --git a/src/foo.rs b/src/foo.rs
--- a/src/foo.rs
+++ b/src/foo.rs
@@ -1,3 +1,4 @@
 a
 b
+c
+d
@@ -20,2 +22,2 @@
 e
 f
diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
@@ -5 +5,2 @@
 old
+new
";
    let lines = diff_new_lines(diff);
    assert_eq!(lines["src/foo.rs"], (1..=4).chain(22..=23).collect());
    assert_eq!(lines["README.md"], (5..=6).collect());
}

#[test]
fn diff_new_lines_ignores_plus_headers_and_skips_files() {
    let diff = "\
+++ b/only-added-content.txt
@@ -0,0 +1,2 @@
+hi
+there
";
    let lines = diff_new_lines(diff);
    assert_eq!(lines["only-added-content.txt"], (1..=2).collect());
}

#[test]
fn anchor_inline_comments_splits_anchored_and_unanchored() {
    let diff = "\
diff --git a/src/foo.rs b/src/foo.rs
--- a/src/foo.rs
+++ b/src/foo.rs
@@ -1,3 +1,4 @@
 a
 b
+c
";
    let lines = diff_new_lines(diff);
    let findings = vec![
        InlineComment {
            path: "src/foo.rs".into(),
            line: 3,
            body: "anchored".into(),
        },
        InlineComment {
            path: "src/foo.rs".into(),
            line: 99,
            body: "beyond hunk".into(),
        },
        InlineComment {
            path: "missing.rs".into(),
            line: 1,
            body: "not in diff".into(),
        },
    ];
    let (anchored, unanchored) = anchor_inline_comments(findings, &lines);
    assert_eq!(anchored.len(), 1);
    assert_eq!(anchored[0].body, "anchored");
    assert_eq!(unanchored.len(), 2);
}
