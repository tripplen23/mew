//! The agent's prompt-building module. The "agent" here is the
//! LLM-facing system prompt, not yet a runtime actor. The
//! [`Harness`](crate::harness::Harness) uses the prompt built here
//! when it talks to the model.

mod prompt;

use mewcode_protocol::Mode;

use crate::skills::SkillRegistry;
use crate::tools::ToolRegistry;

use self::prompt::format_tool_descriptors;

/// Build the system prompt for the given mode.
/// PLAN emphasises analysis, BUILD implementation.
pub fn build_system_prompt(mode: Mode, skills: &SkillRegistry, tools: &ToolRegistry) -> String {
    let mut parts: Vec<String> = Vec::new();

    parts.push(
        "You are an expert software engineer working as a coding assistant inside a terminal application.\n\n\
        The application has two modes the user can switch between:\n\
         - **PLAN** — Read-only analysis and planning. No file modifications.\n\
         - **BUILD** — Full implementation with read and write tools."
            .to_string(),
    );

    if mode == Mode::Plan {
        parts.push(
            "\n## Mode: PLAN\n\
            You are in planning mode. Your job is to analyze, research, and propose solutions — but NOT make changes.\n\
            - Use your available tools to explore the codebase\n\
            - Present your analysis and a clear plan of action\n\
            - Explain trade-offs and ask for clarification when needed"
                .to_string(),
        );
    } else {
        parts.push(
            "\n## Mode: BUILD\n\
            You are in build mode. Your job is to implement changes directly.\n\
            - Read and understand the relevant code before making changes\n\
            - Use write_file to create new files, edit_file for targeted modifications\n\
            - Use bash to run commands (tests, builds, git operations)\n\
            - After making changes, verify the work when possible"
                .to_string(),
        );
    }

    parts.push(
        "\n## Rules\n\
         1. **Be decisive.** Use glob/grep to find what's relevant, then read only those files. Don't read every file in the project.\n\
         2. **Never re-read files you already read** in this conversation.\n\
         3. **Batch your tool calls.** Call multiple tools in parallel when possible (e.g. read 5 files at once, not one at a time).\n\
         4. **Prefer concise responses.** Every tool accepts a `response_format` of `concise` (default) or `detailed`."
            .to_string(),
    );

    // Tools are loaded wholesale (not progressively disclosed) per the
    // Anthropic guide — the model needs the schema to call them.
    let tool_block = format_tool_descriptors(tools);
    if !tool_block.is_empty() {
        parts.push(tool_block);
    }

    // Skills use progressive disclosure: catalog in the prompt, body
    // loaded on demand via `use_skill`.
    let catalog = skills.catalog_for_system_prompt();
    if !catalog.is_empty() {
        parts.push(catalog);
    }

    parts.join("\n")
}
