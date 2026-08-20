use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use mewcode_protocol::{StreamEvent, ToolContracts, ToolDescriptor, ToolError, ToolOutput};
use serde_json::Value;
use tokio::sync::mpsc;
use uuid::Uuid;

use super::ApprovalBroker;

/// Registry of tools available to the harness.
#[derive(Default, Clone)]
pub struct ToolRegistry {
    inner: HashMap<&'static str, Arc<dyn ToolContracts>>,
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolRegistry")
            .field("tools", &self.inner.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl ToolRegistry {
    /// Build an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a tool. A tool with the same name replaces the previous.
    pub fn register(&mut self, tool: Arc<dyn ToolContracts>) {
        self.inner.insert(tool.name(), tool);
    }

    /// Look up a tool by its static name.
    pub fn get_by_name(&self, name: &str) -> Option<Arc<dyn ToolContracts>> {
        self.inner.get(name).cloned()
    }

    /// Names of all registered tools, in insertion order.
    pub fn names(&self) -> Vec<&'static str> {
        self.inner.keys().copied().collect()
    }

    /// `true` if no tools are registered.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Iterate over every registered tool's descriptor.
    pub fn descriptors(&self) -> Vec<ToolDescriptor> {
        self.inner.values().map(|t| t.descriptor()).collect()
    }

    /// Dispatch a tool call. Errors are returned as `ToolErrorPayload`-shaped JSON.
    pub async fn dispatch(&self, name: &str, input: Value) -> ToolOutput {
        match self.inner.get(name) {
            None => ToolError::ToolNotFound(name.to_string()).into(),
            Some(tool) => match tool.execute(input).await {
                Ok(out) => out,
                Err(e) => e.into(),
            },
        }
    }

    /// Return a copy that asks before executing non-read-only Build-mode tools.
    pub fn with_approval(
        &self,
        session_id: Uuid,
        broker: ApprovalBroker,
        events: mpsc::Sender<StreamEvent>,
    ) -> Self {
        let mut reg = ToolRegistry::new();
        for tool in self.inner.values() {
            let annotations = tool.descriptor().annotations;
            if annotations.approval_exempt || annotations.read_only {
                reg.register(tool.clone());
            } else {
                reg.register(Arc::new(ApprovalTool {
                    inner: tool.clone(),
                    session_id,
                    broker: broker.clone(),
                    events: events.clone(),
                }));
            }
        }
        reg
    }
}

struct ApprovalTool {
    inner: Arc<dyn ToolContracts>,
    session_id: Uuid,
    broker: ApprovalBroker,
    events: mpsc::Sender<StreamEvent>,
}

#[async_trait]
impl ToolContracts for ApprovalTool {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn descriptor(&self) -> ToolDescriptor {
        let mut descriptor = self.inner.descriptor();
        descriptor.description = format!(
            "{}\n\n**Approval required:** Build mode asks the user before this tool executes. The user can allow once, allow matching calls in this chat session, or deny.",
            descriptor.description
        );
        descriptor
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError> {
        self.broker
            .approve_tool(self.session_id, self.name(), &input, &self.events)
            .await?;
        self.inner.execute(input).await
    }
}
