//! Architecture canvas: graph + layout data shapes.
//!
//! The canvas model is a "graph is truth, layout is presentation" split:
//! [`Graph`] carries the semantic structure (nodes + edges with their
//! `bind` and `contract` fields) and is the only thing the agent reads,
//! writes, and diffs. [`Layout`] is a pure presentation overlay — node
//! positions and theme — that drift detection ignores entirely. See
//! `docs/architecture-canvas/README.md` §5 for the full design.

use std::collections::HashMap;

/// Stable, opaque node identifier. Renaming a node's `name` never changes
/// its `id`; ids are never reused within a graph.
///
/// Intentionally does not implement `Ord` / `PartialOrd`: ids are opaque,
/// and giving them an ordering would invite callers to rely on insertion
/// order or alphabetical sort, neither of which the design commits to.
///
/// An empty `NodeId` is rejected at deserialize time — the empty string
/// is a real footgun (silent HashMap lookups, broken find-by-id) and the
/// data model has no legitimate use for it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, utoipa::ToSchema)]
#[serde(transparent)]
pub struct NodeId(pub String);

impl NodeId {
    /// Borrow the id as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for NodeId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl<'de> serde::Deserialize<'de> for NodeId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        if s.is_empty() {
            return Err(serde::de::Error::custom(
                "NodeId must not be empty (the empty string breaks HashMap keying and find-by-id)",
            ));
        }
        Ok(Self(s))
    }
}

/// C4-style node taxonomy. We adopt C4's vocabulary (System / Container /
/// Component) so the canvas speaks the same language the rest of the
/// industry does for app architecture.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum NodeKind {
    /// A top-level system (a person or another system sits outside it).
    System,
    /// A runnable thing inside a system (a process, a service, a database).
    Container,
    /// A logical grouping of code inside a container (a module, a bounded
    /// context, a subsystem).
    Component,
}

/// Edge relationships. Kept narrow on purpose — see README §3 decision 2
/// (structure-only sync). Anything semantic or behavioural does not belong
/// here.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum EdgeKind {
    /// Source depends on target (compile-time or runtime).
    Depends,
    /// Source calls into target.
    Calls,
    /// Source implements an interface defined by target.
    Implements,
    /// Source owns / contains target (typically a parent→child relationship).
    Owns,
}

/// A node in the architecture graph. `bind` is null until code is generated
/// or a human binds a node to a real symbol; `contract` is the only field
/// drift detection compares against the bound code.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct Node {
    /// Stable id. Never reused; renaming `name` does not change this.
    pub id: NodeId,
    /// C4 node kind.
    pub kind: NodeKind,
    /// Human-facing display name. May be renamed freely.
    pub name: String,
    /// `path#symbol` binding to a real source symbol. Optional until code
    /// is generated or a human binds the node manually.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bind: Option<String>,
    /// Structural promises (signatures, boundaries, wiring) as
    /// language-neutral strings. Drift detection compares this list against
    /// the bound code's signatures.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contract: Vec<String>,
    /// Optional hint for forward codegen (e.g. `"rust"`, `"python"`).
    /// Ignored by M1 tools.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tech: Option<String>,
    /// Free-text description for humans and the agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
}

/// A directed edge between two nodes of the same graph.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct Edge {
    /// Source node id.
    pub from: NodeId,
    /// Target node id.
    pub to: NodeId,
    /// Relationship kind.
    pub kind: EdgeKind,
}

/// The semantic architecture graph. The single source of truth — what the
/// agent reads, writes, and diffs. This file (and only this file) drives
/// drift detection.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct Graph {
    /// Schema version. Required — a missing `version` is a corrupt file
    /// and fails loud rather than silently defaulting to v0. T2's loader
    /// returns `Graph::default()` only when the *file is absent*, a
    /// different code path from a corrupt file.
    pub version: u32,
    /// Nodes keyed implicitly by their stable `id`. Order is not
    /// significant; serde preserves insertion order on serialize.
    /// `#[serde(default)]` matches the lenient handling of
    /// [`Layout::positions`]: a graph with no nodes is empty, not
    /// corrupt.
    #[serde(default)]
    pub nodes: Vec<Node>,
    /// Edges. Dangling edges (referencing missing nodes) are rejected by
    /// `canvas_mutate` (see milestone-1 T6), not by this struct. Empty
    /// for the same reason as `nodes` above.
    #[serde(default)]
    pub edges: Vec<Edge>,
}

impl Default for Graph {
    fn default() -> Self {
        Self {
            version: 1,
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }
}

/// A 2D point in layout coordinates. Units are abstract (character cells
/// in the TUI render); the layout engine is the only thing that needs to
/// care.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// Theme names the protocol layer understands. The protocol carries the
/// name; the client resolves it to a `Theme` struct (see
/// `ui-aesthetic.md` §4). Keeping this an enum (not a `String`) catches
/// typos at the data-model boundary rather than at theme-load time.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Default,
    serde::Serialize,
    serde::Deserialize,
    utoipa::ToSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeName {
    /// The single default theme shipped in M1. M2 will add more variants
    /// here as the theming surface lands.
    #[default]
    Default,
}

/// Presentation overlay. **Not** the source of truth — drift detection
/// ignores this file entirely. Missing positions are filled by the
/// auto-layout pass; editing this file never triggers codegen.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct Layout {
    pub version: u32,
    /// Map from node id to its resolved position. Absent entries mean
    /// "let auto-layout decide." `#[serde(default)]` is the lenient
    /// counterpart to `Graph.version`'s strictness: a missing
    /// `positions` field is empty, not corrupt.
    ///
    /// Validity (i.e. each key exists in the [`Graph`]) is enforced at
    /// the engine layer (T2's loader), not here. Drift detection (M4)
    /// ignores this field entirely.
    #[serde(default)]
    pub positions: HashMap<NodeId, Point>,
    /// Theme name. Resolved to a `Theme` struct on the client side.
    #[serde(default)]
    pub theme: ThemeName,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            version: 1,
            positions: HashMap::new(),
            theme: ThemeName::default(),
        }
    }
}
