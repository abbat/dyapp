//! Thin projection of common received-pack display controls.
use dyapp_client::sticker_requests::Control;

/// A localized widget choice; native mutation still rechecks its full selection and generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum StickerPackSaveControl {
    /// Start saving the received pack.
    Request,
    /// Retry the retained intent.
    Retry,
    /// Request a fresh descriptor after invalid content.
    Fresh,
    /// Retire acceptance without recalling queued packets.
    StopWaiting,
    /// Read current state instead of repeating an uncertain action.
    Refresh,
    /// Open saved-pack management.
    Manage,
}

impl From<Control> for StickerPackSaveControl {
    fn from(value: Control) -> Self {
        match value {
            Control::Request => Self::Request,
            Control::Retry => Self::Retry,
            Control::Fresh => Self::Fresh,
            Control::StopWaiting => Self::StopWaiting,
            Control::Refresh => Self::Refresh,
            Control::Manage => Self::Manage,
        }
    }
}
