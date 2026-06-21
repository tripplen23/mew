//! System-prompt construction for the mewcode agent.
//!
//! The prompt is assembled from static sections (identity, mode rules) and
//! dynamic sections (tool descriptors, skill catalog). Static text lives in
//! `&'static str` helpers so the prompt layout is readable and easy to edit;
//! dynamic text is generated from the registries passed in.

use std::fmt::Write as _;

use mewcode_protocol::{Mode, ToolDescriptor};

use crate::skills::SkillRegistry;
use crate::tools::ToolRegistry;

/// Build the full system prompt for the given mode.
///
/// PLAN emphasises analysis; BUILD emphasises implementation.
pub fn build_system_prompt(mode: Mode, skills: &SkillRegistry, tools: &ToolRegistry) -> String {
    let mut out = String::new();

    out.push_str(intro());
    out.push_str(mode_section(mode));
    out.push_str(rules());

    let tool_block = format_tool_descriptors(tools);
    if !tool_block.is_empty() {
        out.push('\n');
        out.push_str(&tool_block);
    }

    let catalog = skills.catalog_for_system_prompt();
    if !catalog.is_empty() {
        out.push('\n');
        out.push_str(&catalog);
    }

    out
}

/// Static identity section: who the agent is and what modes exist.
fn intro() -> &'static str {
    "You are an expert software engineer working as a coding assistant inside a terminal application.\n\
     \n\
     The application has two modes the user can switch between:\n\
      - **PLAN** - Read-only analysis and planning. No file modifications.\n\
      - **BUILD** - Full implementation with read and write tools."
}

/// Static mode-specific section.
fn mode_section(mode: Mode) -> &'static str {
    match mode {
        Mode::Plan => {
            "\n\
             \n\
             ## Mode: PLAN\n\
             You are in planning mode. Your job is to analyze, research, and propose solutions - but NOT make changes.\n\
             - Use your available tools to explore the codebase\n\
             - Present your analysis and a clear plan of action\n\
             - Explain trade-offs and ask for clarification when needed"
        }
        Mode::Build => {
            "\n\
             \n\
             ## Mode: BUILD\n\
             You are in build mode. Your job is to implement changes directly.\n\
             - Read and understand the relevant code before making changes\n\
             - Use write_file to create new files, edit_file for targeted modifications\n\
             - Use bash to run commands (tests, builds, git operations)\n\
             - After making changes, verify the work when possible"
        }
    }
}

/// Static rules section.
fn rules() -> &'static str {
    "\n\
     \n\
     ## Rules\n\
      1. **Be decisive.** Use glob/grep to find what's relevant, then read only those files. Don't read every file in the project.\n\
      2. **Never re-read files you already read** in this conversation.\n\
      3. **Batch your tool calls.** Call multiple tools in parallel when possible (e.g. read 5 files at once, not one at a time).\n\
      4. **Prefer concise responses.** Every tool accepts a `response_format` of `concise` (default) or `detailed`."
}

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
        schema, and examples are below - read them carefully before calling a tool. The \
        model is expected to choose the right tool and provide the right parameters.\n\n",
    );

    for d in &descriptors {
        let _ = write!(out, "{}", format_tool_descriptor(d));
    }
    out
}

fn format_tool_descriptor(d: &ToolDescriptor) -> String {
    let mut s = String::new();

    let _ = writeln!(s, "### `{}`\n", d.name);
    let _ = writeln!(s, "{}", d.description.trim());

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

    if flags.is_empty() {
        let _ = writeln!(s, "\n**Max response:** ~{} chars\n", d.max_response_chars);
    } else {
        let _ = writeln!(
            s,
            "\n**Safety:** {} - **Max response:** ~{} chars\n",
            flags.join(", "),
            d.max_response_chars
        );
    }

    let _ = writeln!(s, "**Input schema:**\n```json");
    let _ = writeln!(
        s,
        "{}",
        serde_json::to_string_pretty(&d.input_schema).unwrap_or_else(|_| "{}".into())
    );
    let _ = writeln!(s, "```\n");

    if !d.examples.is_empty() {
        let _ = writeln!(s, "**Examples:**");
        for ex in &d.examples {
            let input = serde_json::to_string(&ex.input).unwrap_or_else(|_| "{}".into());
            let _ = writeln!(s, "- {} -> `{}`", ex.description, input);
        }
        let _ = writeln!(s);
    }

    s
}
