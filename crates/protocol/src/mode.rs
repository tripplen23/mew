use std::str::FromStr;

/// Build vs Plan mode. `Plan` is read-only; `Build` adds write tools.
#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    utoipa::ToSchema,
)]
#[serde(rename_all = "UPPERCASE")]
pub enum Mode {
    /// Full read + write toolset.
    #[default]
    Build,
    /// Read-only analysis and planning.
    Plan,
}

impl Mode {
    /// Lowercase form used in URLs and config keys.
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Build => "build",
            Mode::Plan => "plan",
        }
    }

    /// `true` if write tools should be exposed.
    pub fn allows_writes(self) -> bool {
        matches!(self, Mode::Build)
    }
}

/// Error returned when a string cannot be parsed into a [`Mode`].
#[derive(Debug, thiserror::Error)]
#[error("invalid mode: {0} (expected 'build' or 'plan')")]
pub struct ModeParseError(pub String);

impl FromStr for Mode {
    type Err = ModeParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "build" => Ok(Mode::Build),
            "plan" => Ok(Mode::Plan),
            other => Err(ModeParseError(other.to_string())),
        }
    }
}
