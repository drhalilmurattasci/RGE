//! Unsaved-change discard/cancel confirmation for close-family requests.
//!
//! editor-shell owns only the decision seam. The native dialog implementation
//! lives in the binary and is injected through [`UnsavedChangesDialog`], keeping
//! this crate free of `rfd` or any other native-dialog dependency.

use std::path::{Path, PathBuf};

use super::SaveSource;

/// Close-family request that may need an unsaved-changes confirmation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsavedChangesRequest {
    /// File -> Close.
    CloseFile,
    /// File -> Quit.
    QuitApplication,
    /// Window manager close / title-bar close.
    WindowClose,
}

/// User decision for a dirty close-family request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsavedChangesDecision {
    /// Discard unsaved changes and continue the requested close/quit/exit path.
    Discard,
    /// Keep editing and leave document state unchanged.
    Cancel,
}

/// The kind of save source currently attached to the dirty document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsavedChangesSourceKind {
    /// No save source is attached.
    Unsourced,
    /// A `.rge-scene` source is attached.
    Scene,
    /// A `.rge-project` source is attached.
    Project,
}

/// Read-only dialog context for wording an unsaved-changes prompt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnsavedChangesContext {
    request: UnsavedChangesRequest,
    source_kind: UnsavedChangesSourceKind,
    source_path: Option<PathBuf>,
    source_display_name: Option<String>,
}

impl UnsavedChangesContext {
    /// Build the context from the current source. This clones display data only;
    /// it does not read or mutate the document.
    #[must_use]
    pub fn from_save_source(request: UnsavedChangesRequest, source: Option<&SaveSource>) -> Self {
        let source_kind = match source {
            Some(SaveSource::Scene(_)) => UnsavedChangesSourceKind::Scene,
            Some(SaveSource::Project { .. }) => UnsavedChangesSourceKind::Project,
            None => UnsavedChangesSourceKind::Unsourced,
        };
        let source_path = source.map(|source| source.path().to_path_buf());
        let source_display_name = source
            .and_then(SaveSource::display_name)
            .map(std::string::ToString::to_string);
        Self {
            request,
            source_kind,
            source_path,
            source_display_name,
        }
    }

    /// The close-family request being confirmed.
    #[must_use]
    pub fn request(&self) -> UnsavedChangesRequest {
        self.request
    }

    /// The current source kind.
    #[must_use]
    pub fn source_kind(&self) -> UnsavedChangesSourceKind {
        self.source_kind
    }

    /// The current source path, if one is attached.
    #[must_use]
    pub fn source_path(&self) -> Option<&Path> {
        self.source_path.as_deref()
    }

    /// Human-readable source name for dialog copy, if one is available.
    #[must_use]
    pub fn source_display_name(&self) -> Option<&str> {
        self.source_display_name.as_deref()
    }
}

/// Binary-owned confirmation hook for dirty close-family requests.
///
/// Implementations must return only discard or cancel. They must not save,
/// mutate editor state, mark the command bus saved, or change dirty policy.
pub trait UnsavedChangesDialog {
    /// Ask whether the dirty document should be discarded for `context`.
    fn confirm_discard_unsaved_changes(
        &self,
        context: &UnsavedChangesContext,
    ) -> UnsavedChangesDecision;
}
