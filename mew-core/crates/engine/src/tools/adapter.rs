//! Bridge between mewcode's [`ToolContracts`] trait and Rig's [`ToolDyn`].
//!
//! Rig's agent builder accepts tools that implement `ToolDyn`. Our tools
//! implement mewcode's `ToolContracts` instead (richer descriptors,
//! `ToolError` with hints, `ToolAnnotations`). This adapter wraps any
//! `ToolContracts` implementation so the Rig agent can call it natively
//! during a multi-turn tool-calling loop.
//!
//! The adapter is zero-allocation on the hot path: `definition()` builds
//! a `ToolDefinition` from the cached `ToolDescriptor`, and `call()`
//! delegates to `execute()` then serialises the `ToolOutput` to a JSON
//! string (Rig sends this back to the model as the tool result).

use std::sync::Arc;

use rig_core::tool::{ToolCallExtensions, ToolDyn, ToolExecutionResult, ToolFailure};
use rig_core::wasm_compat::WasmBoxedFuture;

use mewcode_protocol::{ToolContracts, ToolError, ToolErrorPayload};

/// Wrap a mewcode tool so Rig's agent can call it.
pub struct RigToolAdapter {
    /// The mewcode tool being adapted.
    inner: Arc<dyn ToolContracts>,
    /// Cached descriptor — built once at construction.
    descriptor: mewcode_protocol::ToolDescriptor,
}

impl RigToolAdapter {
    /// Wrap a mewcode tool for use with Rig's agent builder.
    pub fn new(inner: Arc<dyn ToolContracts>) -> Self {
        let descriptor = inner.descriptor();
        Self { inner, descriptor }
    }
}

impl ToolDyn for RigToolAdapter {
    fn name(&self) -> String {
        self.inner.name().to_string()
    }

    fn description(&self) -> String {
        self.descriptor.description.clone()
    }

    fn parameters(&self) -> serde_json::Value {
        self.descriptor.input_schema.clone()
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> WasmBoxedFuture<'a, Result<String, rig_core::tool::ToolError>> {
        let inner = self.inner.clone();
        Box::pin(async move {
            let input: serde_json::Value = match serde_json::from_str(&args) {
                Ok(v) => v,
                Err(e) => {
                    // Return an explicit error payload so the model can
                    // correct its tool call instead of getting a confusing
                    // "missing field" message from a silent null.
                    let error = ToolError::InvalidInput {
                        message: format!("invalid JSON arguments: {e}"),
                        hint: Some("check that the arguments are valid JSON".into()),
                    };
                    return Ok(serialize_tool_error(&error));
                }
            };
            match inner.execute(input).await {
                Ok(output) => {
                    // Rig expects a string that the provider sends back as
                    // the tool result content. Serialise the ToolOutput's
                    // inner JSON value to a string.
                    Ok(output.0.to_string())
                }
                Err(e) => {
                    // Serialise the error payload as a successful Ok(String)
                    // so Rig sends it back to the model as the tool result.
                    // The model sees the error kind + hint and can retry
                    // with corrected input.
                    Ok(serialize_tool_error(&e))
                }
            }
        })
    }

    fn call_structured<'a>(
        &'a self,
        args: String,
        _extensions: &'a ToolCallExtensions,
    ) -> WasmBoxedFuture<'a, ToolExecutionResult> {
        let inner = self.inner.clone();
        Box::pin(async move {
            let input: serde_json::Value = match serde_json::from_str(&args) {
                Ok(v) => v,
                Err(e) => {
                    let error = ToolError::InvalidInput {
                        message: format!("invalid JSON arguments: {e}"),
                        hint: Some("check that the arguments are valid JSON".into()),
                    };
                    return ToolExecutionResult::failed(
                        serialize_tool_error(&error),
                        failure_for_tool_error(&error),
                    );
                }
            };
            match inner.execute(input).await {
                Ok(output) => ToolExecutionResult::success(output.0.to_string()),
                Err(error @ ToolError::Rejected { .. }) => {
                    ToolExecutionResult::denied(serialize_tool_error(&error))
                }
                Err(error) => ToolExecutionResult::failed(
                    serialize_tool_error(&error),
                    failure_for_tool_error(&error),
                ),
            }
        })
    }
}

fn serialize_tool_error(error: &ToolError) -> String {
    let payload: ToolErrorPayload = error.into();
    serde_json::to_string(&payload)
        .unwrap_or_else(|_| r#"{"error":true,"kind":"other","message":"tool failed"}"#.to_string())
}

fn failure_for_tool_error(error: &ToolError) -> ToolFailure {
    match error {
        ToolError::InvalidInput { message, .. } => ToolFailure::invalid_args(message),
        ToolError::Io(error) => match error.kind() {
            std::io::ErrorKind::NotFound => ToolFailure::not_found(error.to_string()),
            std::io::ErrorKind::PermissionDenied => {
                ToolFailure::permission_denied(error.to_string())
            }
            std::io::ErrorKind::TimedOut => ToolFailure::timeout(error.to_string()),
            std::io::ErrorKind::Interrupted => ToolFailure::cancelled(error.to_string()),
            _ => ToolFailure::other(error.to_string()).with_retryable(true),
        },
        ToolError::Rejected { message, .. } => ToolFailure::permission_denied(message),
        ToolError::ToolNotFound(name) => {
            ToolFailure::not_found(format!("tool '{name}' is not registered"))
        }
        ToolError::Other { message, .. } => ToolFailure::other(message),
    }
}

/// Convert a [`ToolRegistry`](crate::tools::ToolRegistry) into the
/// `Vec<Box<dyn ToolDyn>>` that Rig's agent builder expects.
pub fn rig_tools(registry: &crate::tools::ToolRegistry) -> Vec<Box<dyn ToolDyn>> {
    let descriptors = registry.descriptors();
    let mut names: Vec<&str> = descriptors.iter().map(|d| d.name.as_str()).collect();
    names.sort();
    names
        .into_iter()
        .filter_map(|name| registry.get_by_name(name))
        .map(|tool| Box::new(RigToolAdapter::new(tool)) as Box<dyn ToolDyn>)
        .collect()
}
