/// One row in the slash-command picker.
#[derive(Debug, Clone, Copy)]
pub struct SlashCommand {
    pub kind: SlashCommandKind,
    /// What to seed the composer with, e.g. `"/model"`.
    pub command: &'static str,
    /// Single-line explanation shown next to the row.
    pub description: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlashCommandKind {
    Model,
    Session,
    Tools,
    Skills,
    Theme,
    Mode,
    Sound,
    Connect,
    Compact,
    Quit,
}

impl SlashCommand {
    pub fn token(self) -> &'static str {
        self.command.trim_start_matches('/')
    }
}

/// Catalog of slash commands surfaced in the picker.
pub const SLASH_COMMANDS: &[SlashCommand] = &[
    SlashCommand {
        kind: SlashCommandKind::Model,
        command: "/model",
        description: "Switch model",
    },
    SlashCommand {
        kind: SlashCommandKind::Session,
        command: "/session",
        description: "List sessions",
    },
    SlashCommand {
        kind: SlashCommandKind::Session,
        command: "/session new",
        description: "Create a new session",
    },
    SlashCommand {
        kind: SlashCommandKind::Session,
        command: "/session rename",
        description: "Rename current session",
    },
    SlashCommand {
        kind: SlashCommandKind::Tools,
        command: "/tools",
        description: "List tools",
    },
    SlashCommand {
        kind: SlashCommandKind::Skills,
        command: "/skills",
        description: "List skills",
    },
    SlashCommand {
        kind: SlashCommandKind::Theme,
        command: "/theme",
        description: "Pick theme",
    },
    SlashCommand {
        kind: SlashCommandKind::Mode,
        command: "/mode",
        description: "Switch mode",
    },
    SlashCommand {
        kind: SlashCommandKind::Sound,
        command: "/sound",
        description: "Toggle notification sound",
    },
    SlashCommand {
        kind: SlashCommandKind::Connect,
        command: "/connect",
        description: "Connect a provider",
    },
    SlashCommand {
        kind: SlashCommandKind::Compact,
        command: "/compact",
        description: "Compact conversation context",
    },
    SlashCommand {
        kind: SlashCommandKind::Quit,
        command: "quit",
        description: "Exit the TUI",
    },
];

pub fn slash_command_by_token(token: &str) -> Option<SlashCommand> {
    SLASH_COMMANDS
        .iter()
        .copied()
        .filter(|command| command.command.starts_with('/'))
        .find(|command| command.token() == token)
}
