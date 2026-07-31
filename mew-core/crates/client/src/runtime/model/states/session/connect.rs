use tui_textarea::TextArea;

use mewcode_protocol::ProviderId;

/// State for the provider connect dialog.
#[derive(Default)]
pub struct ConnectProviderState {
    /// Which step in the wizard: picking provider, entering key, or awaiting validation.
    pub step: ConnectStep,
    /// Which provider the user selected.
    pub selected_provider: Option<ProviderId>,
    /// Error message from validation, if any.
    pub error: Option<String>,
    /// Inline text input for the key entry step (masked in UI).
    pub key_input: TextArea<'static>,
    /// Bumped on each submission to discard stale responses.
    pub attempt: u64,
}

impl std::fmt::Debug for ConnectProviderState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectProviderState")
            .field("step", &self.step)
            .field("selected_provider", &self.selected_provider)
            .field("error", &self.error)
            .finish()
    }
}

/// Steps in the provider connect wizard.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ConnectStep {
    /// Pick a provider from the list.
    #[default]
    PickProvider,
    /// Enter the API key.
    EnterKey,
    /// Waiting for server validation.
    Validating,
    /// Connected successfully.
    Done,
}
