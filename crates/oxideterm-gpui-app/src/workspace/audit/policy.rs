use super::*;
use oxideterm_audit::AuditPolicy;

const MIB: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::workspace) enum AuditPolicyInput {
    Retention,
    Capacity,
}

pub(super) struct AuditPolicyDraft {
    output: bool,
    retention: String,
    capacity: String,
    invalid: bool,
}

impl AuditPolicyDraft {
    fn new(policy: AuditPolicy, output: bool) -> Self {
        let (days, bytes) = if output {
            (policy.output_retention_days, policy.output_max_bytes)
        } else {
            (policy.retention_days, policy.max_bytes)
        };
        Self {
            output,
            retention: days.to_string(),
            capacity: bytes.div_ceil(MIB).to_string(),
            invalid: false,
        }
    }

    fn value(&self, input: AuditPolicyInput) -> &str {
        match input {
            AuditPolicyInput::Retention => &self.retention,
            AuditPolicyInput::Capacity => &self.capacity,
        }
    }

    pub(super) fn apply(&self, mut policy: AuditPolicy) -> Option<AuditPolicy> {
        let integer = |value: &str| {
            let value = value.trim();
            (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
                .then(|| value.parse::<u64>().ok())
                .flatten()
        };
        let days = integer(&self.retention)?;
        let mib = integer(&self.capacity)?;
        if !(u64::from(AuditPolicy::MIN_RETENTION_DAYS)
            ..=u64::from(AuditPolicy::MAX_RETENTION_DAYS))
            .contains(&days)
            || !(AuditPolicy::MIN_BYTES / MIB..=AuditPolicy::MAX_BYTES / MIB).contains(&mib)
        {
            return None;
        }
        let (retention, capacity) = if self.output {
            (
                &mut policy.output_retention_days,
                &mut policy.output_max_bytes,
            )
        } else {
            (&mut policy.retention_days, &mut policy.max_bytes)
        };
        *retention = days as u32;
        // Preserve an existing byte-exact limit when its displayed MiB value is unchanged.
        if mib != capacity.div_ceil(MIB) {
            *capacity = mib * MIB;
        }
        Some(policy)
    }
}

impl WorkspaceApp {
    fn audit_policy_draft(&self) -> AuditPolicyDraft {
        AuditPolicyDraft::new(self.audit.policy, self.audit.view == AuditView::Recordings)
    }

    pub(in crate::workspace) fn audit_policy_input_value(&self, input: AuditPolicyInput) -> String {
        self.audit
            .policy_draft
            .as_ref()
            .map(|draft| draft.value(input).to_owned())
            .unwrap_or_else(|| self.audit_policy_draft().value(input).to_owned())
    }

    pub(in crate::workspace) fn replace_audit_policy_input(
        &mut self,
        input: AuditPolicyInput,
        range: Option<std::ops::Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        if self.audit.policy_draft.is_none() {
            self.audit.policy_draft = Some(self.audit_policy_draft());
        }
        let draft = self.audit.policy_draft.as_mut().unwrap();
        let value = match input {
            AuditPolicyInput::Retention => &mut draft.retention,
            AuditPolicyInput::Capacity => &mut draft.capacity,
        };
        oxideterm_editor_core::utf16::replace_utf16(value, range, text);
        draft.invalid = false;
        self.show_active_input_caret(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn apply_audit_policy_inputs(&mut self, cx: &mut Context<Self>) {
        if self.audit.settings_task.is_some() || self.audit.loading {
            return;
        }
        let Some(draft) = self.audit.policy_draft.as_mut() else {
            return;
        };
        let Some(policy) = draft.apply(self.audit.policy) else {
            draft.invalid = true;
            cx.notify();
            return;
        };
        self.save_audit_policy(policy, cx);
    }

    fn render_audit_policy_input(&self, input: AuditPolicyInput, cx: &Context<Self>) -> AnyElement {
        let target = WorkspaceImeTarget::AuditPolicy(input);
        let output = self.audit.view == AuditView::Recordings;
        let key = match (input, output) {
            (AuditPolicyInput::Retention, false) => "event_log.audit.retention",
            (AuditPolicyInput::Capacity, false) => "event_log.audit.capacity",
            (AuditPolicyInput::Retention, true) => "event_log.recordings.retention",
            (AuditPolicyInput::Capacity, true) => "event_log.recordings.capacity",
        };
        let value = self.audit_policy_input_value(input);
        let field = oxideterm_gpui_ui::text_input::text_input(
            &self.tokens,
            oxideterm_gpui_ui::text_input::TextInputView {
                value: &value,
                placeholder: String::new(),
                focused: self.active_ime_target(cx) == Some(target),
                caret_visible: self.input_caret.visible(),
                secret: false,
                selected_all: false,
                selected_range: self.ime_selected_range_for_target(target, cx),
                marked_text: self.marked_text_for_target(target, cx),
            },
        )
        .w(px(88.0))
        .h(px(28.0));
        let field = self.text_input_with_workspace_ime(
            target,
            field,
            move |this, cx| {
                if this.audit.policy_draft.is_none() {
                    this.audit.policy_draft = Some(this.audit_policy_draft());
                }
                this.selected_ime_target = Some(target);
                this.show_active_input_caret(cx);
            },
            cx,
        );
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(self.i18n.t(key))
            .child(field)
            .child(if input == AuditPolicyInput::Retention {
                self.i18n.t("event_log.audit.days_unit")
            } else {
                "MiB".to_owned()
            })
            .into_any_element()
    }

    pub(super) fn render_audit_policy_inputs(&self, cx: &Context<Self>) -> AnyElement {
        let invalid = self
            .audit
            .policy_draft
            .as_ref()
            .is_some_and(|draft| draft.invalid);
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_3()
                    .child(self.render_audit_policy_input(AuditPolicyInput::Retention, cx))
                    .child(self.render_audit_policy_input(AuditPolicyInput::Capacity, cx))
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("event_log.audit.save_limits"),
                        None,
                        ToolbarButtonOptions {
                            button: ButtonOptions {
                                variant: ButtonVariant::Outline,
                                size: ButtonSize::Sm,
                                radius: ButtonRadius::Sm,
                                disabled: self.audit.policy_draft.is_none()
                                    || self.audit.settings_task.is_some()
                                    || self.audit.loading
                                    || self.audit.client.is_none(),
                            },
                            ..Default::default()
                        },
                        cx.listener(|this, _, _, cx| this.apply_audit_policy_inputs(cx)),
                    )),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(if invalid {
                        self.tokens.ui.error
                    } else {
                        self.tokens.ui.text_muted
                    }))
                    .child(
                        self.i18n
                            .t(if invalid {
                                "event_log.audit.limits_invalid"
                            } else {
                                "event_log.audit.limits_hint"
                            })
                            .replace("{{minDays}}", &AuditPolicy::MIN_RETENTION_DAYS.to_string())
                            .replace("{{maxDays}}", &AuditPolicy::MAX_RETENTION_DAYS.to_string())
                            .replace("{{minMiB}}", &(AuditPolicy::MIN_BYTES / MIB).to_string())
                            .replace("{{maxMiB}}", &(AuditPolicy::MAX_BYTES / MIB).to_string()),
                    ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_limits_validate_units_and_preserve_other_policy_fields() {
        let original = AuditPolicy {
            enabled: true,
            record_output: true,
            ..Default::default()
        };
        for output in [false, true] {
            let mut draft = AuditPolicyDraft::new(original, output);
            for (days, capacity, expected_days, expected_bytes) in [
                (" 123 ", "333", 123, 349_175_808),
                ("1", "1", 1, 1_048_576),
                ("3650", "65536", 3650, 68_719_476_736),
            ] {
                draft.retention = days.into();
                draft.capacity = capacity.into();
                let mut expected = original;
                if output {
                    expected.output_retention_days = expected_days;
                    expected.output_max_bytes = expected_bytes;
                } else {
                    expected.retention_days = expected_days;
                    expected.max_bytes = expected_bytes;
                }
                assert_eq!(draft.apply(original), Some(expected));
            }
            for (days, capacity) in [
                ("", "333"),
                ("0", "333"),
                ("3651", "333"),
                ("123", "0"),
                ("123", "65537"),
                ("123", "1.5"),
                ("123", "18446744073709551616"),
            ] {
                draft.retention = days.into();
                draft.capacity = capacity.into();
                assert_eq!(draft.apply(original), None, "{days} / {capacity}");
            }
        }
        let byte_limit = AuditPolicy {
            max_bytes: 1_048_577,
            ..original
        };
        let mut draft = AuditPolicyDraft::new(byte_limit, false);
        draft.retention = "12".into();
        assert_eq!(
            draft.apply(byte_limit),
            Some(AuditPolicy {
                retention_days: 12,
                ..byte_limit
            })
        );
    }
}
