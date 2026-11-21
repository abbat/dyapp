//! Shared display controls; current-message and generation checks still authorize each mutation.
use super::{Failure, State, Status};

/// An offered received-pack control, independent of widget or localized label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    /// Start saving this received pack.
    Request,
    /// Retry the current retained intent.
    Retry,
    /// Request a fresh descriptor after invalid content.
    Fresh,
    /// Stop accepting this intent without recalling queued packets.
    StopWaiting,
    /// Read current status without repeating an uncertain mutation.
    Refresh,
    /// Open the saved-pack collection.
    Manage,
}

impl Status {
    /// Project controls in their established display order, without interpreting client clocks.
    /// These hints never replace current store, full-message or generation authorization.
    #[must_use]
    pub fn controls(&self) -> Vec<Control> {
        let mut controls = Vec::new();
        if self.state == State::New {
            controls.push(Control::Request);
        }
        if self.state == State::Failed {
            if self.failure != Some(Failure::StickerMissing) {
                controls.push(if self.generation.is_some() {
                    Control::Retry
                } else {
                    Control::Refresh
                });
            }
            if self.generation.is_some() && self.failure == Some(Failure::Invalid) {
                controls.push(Control::Fresh);
            }
        }
        if self.generation.is_some() {
            controls.push(Control::StopWaiting);
        }
        if self.state == State::Saved
            || matches!(
                self.failure,
                Some(Failure::Quota | Failure::Storage | Failure::StickerMissing)
            )
        {
            controls.push(Control::Manage);
        }
        controls
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Control::{Fresh, Manage, Refresh, Request, Retry, StopWaiting},
        Failure::{Fetch, Invalid, Quota, StickerMissing, Storage},
        State::{Downloading, Failed, New, Saved, Waiting},
        Status,
    };

    #[test]
    fn controls_preserve_the_received_pack_action_table_and_order() {
        let cases = [
            (New, None, false, vec![Request]),
            (Saved, None, false, vec![Manage]),
            (Waiting, None, true, vec![StopWaiting]),
            (Downloading, None, true, vec![StopWaiting]),
            (Failed, Some(Fetch), true, vec![Retry, StopWaiting]),
            (Failed, Some(Invalid), true, vec![Retry, Fresh, StopWaiting]),
            (Failed, Some(Quota), true, vec![Retry, StopWaiting, Manage]),
            (
                Failed,
                Some(Storage),
                true,
                vec![Retry, StopWaiting, Manage],
            ),
            (
                Failed,
                Some(StickerMissing),
                true,
                vec![StopWaiting, Manage],
            ),
            (Failed, Some(Fetch), false, vec![Refresh]),
            (Failed, Some(Invalid), false, vec![Refresh]),
            (Failed, Some(Quota), false, vec![Refresh, Manage]),
            (Failed, Some(Storage), false, vec![Refresh, Manage]),
            (Failed, Some(StickerMissing), false, vec![Manage]),
        ];
        for (state, failure, pending, expected) in cases {
            let status = Status {
                state,
                generation: pending.then_some([1; 16]),
                failure,
                retry_at_ms: None,
            };
            assert_eq!(status.controls(), expected, "{status:?}");
            for deadline in [0, 1, u64::MAX] {
                assert_eq!(
                    Status {
                        retry_at_ms: Some(deadline),
                        ..status.clone()
                    }
                    .controls(),
                    expected,
                    "display controls must not guess deadline eligibility"
                );
            }
        }
    }
}
