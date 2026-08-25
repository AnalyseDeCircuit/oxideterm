use cocoa::base::id;
use gpui::HapticFeedbackStyle;
use objc::{class, msg_send, sel, sel_impl};

/// macOS haptic feedback using [`NSHapticFeedbackManager`].
///
/// Delivers transient taps through the Force Touch trackpad (macOS 10.11+).
/// Fire and forget. On machines without haptic hardware, calls are silently ignored by AppKit.
pub(crate) struct MacHaptics {
    supported: bool,
}

/// <https://developer.apple.com/documentation/appkit/nshapticfeedbackmanager/feedbackpattern>
#[allow(dead_code)]
mod feedback_pattern {
    pub const GENERIC: isize = 0;
    pub const ALIGNMENT: isize = 1;
    pub const LEVEL_CHANGE: isize = 2;
}

impl MacHaptics {
    pub fn new(headless: bool) -> Self {
        Self {
            supported: !headless,
        }
    }

    pub fn supported(&self) -> bool {
        self.supported
    }

    fn pattern_for_style(style: HapticFeedbackStyle) -> isize {
        match style {
            HapticFeedbackStyle::Generic => feedback_pattern::GENERIC,
            HapticFeedbackStyle::Alignment => feedback_pattern::ALIGNMENT,
            HapticFeedbackStyle::LevelChange => feedback_pattern::LEVEL_CHANGE,
        }
    }

    pub fn play(&self, style: HapticFeedbackStyle) {
        if !self.supported {
            return;
        }

        let pattern = Self::pattern_for_style(style);

        /// <https://developer.apple.com/documentation/appkit/nshapticfeedbackmanager/performancetime>
        const PERFORMANCE_TIME_NOW: usize = 1;

        // Safety: NSHapticFeedbackManager is always available on macOS 10.11+.
        // All Platform trait methods run on the main thread.
        unsafe {
            let manager: id = msg_send![class!(NSHapticFeedbackManager), defaultPerformer];
            let _: () = msg_send![
                manager,
                performFeedbackPattern: pattern
                performanceTime: PERFORMANCE_TIME_NOW
            ];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supported() {
        let haptics = MacHaptics::new(false);
        assert!(haptics.supported());
    }

    #[test]
    fn test_headless_is_unsupported() {
        let haptics = MacHaptics::new(true);
        assert!(!haptics.supported());
    }

    #[test]
    fn test_style_to_pattern_mapping() {
        assert_eq!(
            MacHaptics::pattern_for_style(HapticFeedbackStyle::Generic),
            feedback_pattern::GENERIC
        );
        assert_eq!(
            MacHaptics::pattern_for_style(HapticFeedbackStyle::Alignment),
            feedback_pattern::ALIGNMENT
        );
        assert_eq!(
            MacHaptics::pattern_for_style(HapticFeedbackStyle::LevelChange),
            feedback_pattern::LEVEL_CHANGE
        );
    }
}
