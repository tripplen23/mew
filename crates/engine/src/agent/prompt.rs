//! Prompt rendering helpers. Turns a `ToolDescriptor` (and a list of
//! them) into the markdown block injected into the system prompt.

use mewcode_protocol::ToolDescriptor;

use crate::tools::ToolRegistry;

/// Render the full set of tool descriptors as a markdown block for the
/// system prompt, sorted alphabetically by name. Empty string if the
/// registry is empty.
pub fn format_tool_descriptors(tools: &ToolRegistry) -> String {
    if tools.is_empty() {
        return String::new();
    }
    let mut descriptors = tools.descriptors();
    descriptors.sort_by(|a, b| a.name.cmp(&b.name));

    let mut out = String::from("\n## Tool reference\n\n");
    out.push_str(
        "The following tools are available in every turn. Each tool's description, input \
        schema, and examples are below — read them carefully before calling a tool. The \
        model is expected to choose the right tool and provide the right parameters.\n\n",
    );

    for d in &descriptors {
        out.push_str(&format_tool_descriptor(d));
        out.push('\n');
    }
    out
}

fn format_tool_descriptor(d: &ToolDescriptor) -> String {
    let mut s = String::new();

    s.push_str(&format!("### `{}`\n\n", d.name));
    s.push_str(d.description.trim());
    s.push_str("\n\n");

    // Annotations as a compact one-liner; absent flags are simply skipped.
    let mut flags = Vec::new();
    if d.annotations.read_only {
        flags.push("read-only");
    }
    if d.annotations.destructive {
        flags.push("destructive");
    }
    if d.annotations.open_world {
        flags.push("open-world");
    }
    if d.annotations.idempotent {
        flags.push("idempotent");
    }
    if !flags.is_empty() {
        s.push_str(&format!(
            "**Safety:** {} · **Max response:** ~{} chars\n\n",
            flags.join(", "),
            d.max_response_chars
        ));
    } else {
        s.push_str(&format!(
            "**Max response:** ~{} chars\n\n",
            d.max_response_chars
        ));
    }

    s.push_str("**Input schema:**\n```json\n");
    s.push_str(&serde_json::to_string_pretty(&d.input_schema).unwrap_or_else(|_| "{}".into()));
    s.push_str("\n```\n\n");

    if !d.examples.is_empty() {
        s.push_str("**Examples:**\n");
        for ex in &d.examples {
            let input = serde_json::to_string(&ex.input).unwrap_or_else(|_| "{}".into());
            s.push_str(&format!("- {} → `{}`\n", ex.description, input));
        }
        s.push('\n');
    }

    s
}
