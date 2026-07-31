use std::time::Instant;

use mewcode_protocol::event::{ChoiceCancelReason, ChoiceRequest, ChoiceResponse};

use super::PickerState;

/// Pending structured choice prompt (single-select) awaiting a response.
#[derive(Debug)]
pub struct ChoicePromptState {
    pub request: ChoiceRequest,
    pub picker: PickerState,
    pub started_at: Instant,
    pub response: Option<ChoiceResponse>,
}

impl ChoicePromptState {
    pub fn new(request: ChoiceRequest) -> Self {
        Self {
            request,
            picker: PickerState::default(),
            started_at: Instant::now(),
            response: None,
        }
    }

    pub fn cancel(&mut self, reason: ChoiceCancelReason) {
        self.response = Some(ChoiceResponse::Cancelled {
            request_id: self.request.request_id.clone(),
            reason,
        });
    }
}
