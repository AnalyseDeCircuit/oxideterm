#[derive(Clone, Copy, Debug)]
pub struct FocusCycle<'a, T> {
    actions: &'a [T],
}

impl<'a, T> FocusCycle<'a, T>
where
    T: Copy + Eq,
{
    pub const fn new(actions: &'a [T]) -> Self {
        Self { actions }
    }

    pub fn next(self, current: Option<T>, forward: bool) -> Option<T> {
        // GPUI does not provide the browser/Radix footer tab loop. Keep the
        // wrapping action order in one tested helper instead of duplicating it
        // in every modal, select, and recorder footer.
        let Some(first) = self.actions.first().copied() else {
            return None;
        };
        let last = self.actions.last().copied().unwrap_or(first);
        let Some(current) = current else {
            return Some(if forward { first } else { last });
        };
        let Some(index) = self
            .actions
            .iter()
            .position(|candidate| *candidate == current)
        else {
            return Some(if forward { first } else { last });
        };

        if forward {
            self.actions.get(index + 1).copied().or(Some(first))
        } else {
            index
                .checked_sub(1)
                .and_then(|previous| self.actions.get(previous).copied())
                .or(Some(last))
        }
    }
}

pub fn next_modal_footer_focus<T>(actions: &[T], current: Option<T>, forward: bool) -> Option<T>
where
    T: Copy + Eq,
{
    // Radix/Dialog footer buttons follow DOM tab order even when buttons are
    // conditionally hidden. Keep modal footers on this explicit entry point so
    // settings, AI, keybinding, and import/export dialogs do not reimplement
    // their own wrapping rules.
    FocusCycle::new(actions).next(current, forward)
}

pub fn next_required_modal_footer_focus<T>(
    actions: &[T],
    current: Option<T>,
    forward: bool,
    fallback: T,
) -> T
where
    T: Copy + Eq,
{
    next_modal_footer_focus(actions, current, forward).unwrap_or(fallback)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModalFooterKeyAction<T> {
    Cancel,
    Focus(T),
    Activate(T),
}

pub fn modal_footer_key_action<T>(
    key: &str,
    shift: bool,
    actions: &[T],
    current: Option<T>,
    fallback: T,
) -> Option<ModalFooterKeyAction<T>>
where
    T: Copy + Eq,
{
    // Dialog footer key handling has the same browser contract across standard
    // confirms, Cloud Sync confirms, keybinding recorder, and .oxide
    // import/export: Escape closes, Tab/arrows move focus, Home/End jump to
    // the footer edges, and Enter/Space activates the focused action.
    match key {
        "escape" => Some(ModalFooterKeyAction::Cancel),
        "tab" | "arrowleft" | "left" | "arrowright" | "right" => {
            let forward = modal_footer_key_moves_forward(key, shift);
            Some(ModalFooterKeyAction::Focus(
                next_required_modal_footer_focus(actions, current, forward, fallback),
            ))
        }
        "home" => actions
            .first()
            .copied()
            .or(Some(fallback))
            .map(ModalFooterKeyAction::Focus),
        "end" => actions
            .last()
            .copied()
            .or(Some(fallback))
            .map(ModalFooterKeyAction::Focus),
        "enter" | "space" | " " => {
            Some(ModalFooterKeyAction::Activate(current.unwrap_or(fallback)))
        }
        _ => None,
    }
}

pub fn modal_footer_key_moves_forward(key: &str, shift: bool) -> bool {
    // Browser/Radix dialogs let Shift+Tab and left-arrow walk backward through
    // footer actions. Keep key-direction mapping shared so standard confirms,
    // Cloud Sync confirms, and import/export modals do not drift apart.
    !shift && !matches!(key, "arrowleft" | "left")
}
