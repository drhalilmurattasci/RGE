//! `editor_ui::widgets` — collection of egui-backed editor widgets.
//!
//! - [`node_graph`] — domain-agnostic graph visualization (W08); consumes
//!   `&dyn rge_kernel_graph_foundation::VizAdapter`.
//! - [`inspector`] — Phase 9 read-only editor-session inspector;
//!   consumes `&rge_editor_state::InspectorSnapshot`.

pub mod inspector;
pub mod node_graph;
