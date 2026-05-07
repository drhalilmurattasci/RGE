//! `rge-editor-ui` — editor UI subsystem.
//!
//! Failure class: recoverable
//!
//! Per PLAN §1.13: editor-ui failures (dock layout migration error, workspace
//! load failure, missing menu binding, widget render fault) are transient and
//! recoverable in-place — the editor falls back to default layout, surfaces a
//! diagnostic, or skips the offending widget. No PIE state is owned here;
//! editor coordination state lives in `crates/editor-state`. Matches gfx +
//! ui-theme + ui-fonts (UI substrate classification).
//!
//! Per [PLAN.md §6](../../PLAN.md). Adapts UE Slate patterns (UToolMenus, FTabManager,
//! FLayoutSaveRestore, FSlateStyleSet) to egui via `egui_dock`.

pub mod dock;
pub mod layout;
pub mod menus;
pub mod plugin_adapter;
pub mod widgets;
pub mod workspace;

pub use plugin_adapter::{EditorUiPlugin, EDITOR_UI_PLUGIN_ID};
