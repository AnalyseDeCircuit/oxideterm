use super::*;
use crate::form_state::{
    clear_current_connection_field, insert_text_into_current_connection_field,
    toggle_connection_secret_field_visibility,
};
use crate::{NewConnectionForm, SshAuthTab};
use gpui::{AppContext, TestAppContext};
use tokio::sync::oneshot;

fn saved_form() -> NewConnectionForm {
    let mut form = NewConnectionForm::default();
    form.saved_password_keychain_id = Some("test-password-owner".into());
    form.password_loaded = false;
    form.auth_tab = SshAuthTab::Password;
    form.focused_field = NewConnectionField::Password;
    form
}

#[gpui::test]
fn saved_password_reveal_and_hide_preserve_the_stored_reference(cx: &mut TestAppContext) {
    let entity = cx.new(ConnectionFlowEntity::new);
    entity.update(cx, |entity, cx| {
        entity.form.replace_with_new_form(saved_form());
        assert!(
            entity.start_password_load(async { Ok(SecretString::from("saved-test-secret")) }, cx)
        );
    });
    cx.run_until_parked();
    entity.update(cx, |entity, _| {
        let form = entity.form.form.as_mut().unwrap();
        assert_eq!(form.password, "saved-test-secret");
        assert!(form.password_loaded && form.password_visible && form.password_from_store);
        assert!(!format!("{form:?}").contains("saved-test-secret"));
        toggle_connection_secret_field_visibility(form, NewConnectionField::Password);
        assert!(form.password.is_empty());
        assert!(!form.password_loaded && !form.password_visible);
        assert_eq!(
            form.saved_password_keychain_id.as_deref(),
            Some("test-password-owner")
        );
    });
}

#[gpui::test]
fn saved_password_load_cannot_overwrite_edits_or_a_replacement_form(cx: &mut TestAppContext) {
    for scenario in 0..3 {
        let entity = cx.new(ConnectionFlowEntity::new);
        let (tx, rx) = oneshot::channel();
        entity.update(cx, |entity, cx| {
            entity.form.replace_with_new_form(saved_form());
            entity.start_password_load(async move { rx.await.unwrap() }, cx);
        });
        cx.run_until_parked();
        entity.update(cx, |entity, _| {
            if scenario == 2 {
                entity.form.replace_with_new_form(saved_form());
            } else {
                let form = entity.form.form.as_mut().unwrap();
                insert_text_into_current_connection_field(form, "replacement");
                if scenario == 1 {
                    clear_current_connection_field(form);
                    form.password_loaded = false;
                }
            }
        });
        tx.send(Ok(SecretString::from("stale-test-secret")))
            .unwrap();
        cx.run_until_parked();
        entity.read_with(cx, |entity, _| {
            let form = entity.form.form.as_ref().unwrap();
            assert_eq!(
                form.password,
                if scenario == 0 { "replacement" } else { "" }
            );
            assert!(!form.password_from_store);
        });
    }
}

#[gpui::test]
fn closing_form_cancels_pending_password_load_before_exit_animation(cx: &mut TestAppContext) {
    let entity = cx.new(ConnectionFlowEntity::new);
    let (tx, rx) = oneshot::channel::<Result<SecretString, SavedPasswordLoadError>>();
    entity.update(cx, |entity, cx| {
        entity.form.replace_with_new_form(saved_form());
        entity.start_password_load(async move { rx.await.unwrap() }, cx);
    });
    cx.run_until_parked();
    entity.update(cx, |entity, cx| {
        entity.begin_connection_form_exit(Duration::from_millis(200), cx);
        assert!(entity.password_load_task.is_none());
        assert!(
            entity
                .form
                .form
                .as_ref()
                .unwrap()
                .password_load_id
                .is_none()
        );
    });
    cx.run_until_parked();
    assert!(tx.is_closed());
}

#[gpui::test]
fn saved_password_read_failure_can_be_retried(cx: &mut TestAppContext) {
    for failure in [
        SavedPasswordLoadError::NotFound,
        SavedPasswordLoadError::Locked,
        SavedPasswordLoadError::Unavailable,
    ] {
        let entity = cx.new(ConnectionFlowEntity::new);
        entity.update(cx, |entity, cx| {
            entity.form.replace_with_new_form(saved_form());
            entity.start_password_load(async move { Err(failure) }, cx);
        });
        cx.run_until_parked();
        entity.update(cx, |entity, cx| {
            let form = entity.form.form.as_ref().unwrap();
            assert_eq!(form.password_load_error, Some(failure));
            assert!(form.password.is_empty() && !form.password_loaded);
            assert!(
                entity.start_password_load(async { Ok(SecretString::from("retry-secret")) }, cx)
            );
        });
        cx.run_until_parked();
        entity.read_with(cx, |entity, _| {
            let form = entity.form.form.as_ref().unwrap();
            assert_eq!(form.password, "retry-secret");
            assert!(form.password_load_error.is_none());
        });
    }
}
