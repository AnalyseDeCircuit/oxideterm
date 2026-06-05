#[cfg(target_os = "macos")]
mod macos {
    use std::sync::mpsc;

    use objc2::rc::Retained;
    use objc2::runtime::Bool;
    use objc2::{class, msg_send};
    use objc2_foundation::{NSError, NSString};

    #[link(name = "LocalAuthentication", kind = "framework")]
    unsafe extern "C" {}

    const LA_POLICY_BIOMETRICS: i64 = 1;
    const LA_POLICY_DEVICE_OWNER: i64 = 2;

    pub fn is_biometric_available() -> bool {
        unsafe {
            let cls = class!(LAContext);
            let ctx: *mut objc2::runtime::AnyObject = msg_send![cls, alloc];
            let ctx: *mut objc2::runtime::AnyObject = msg_send![ctx, init];
            if ctx.is_null() {
                return false;
            }
            let can_eval: Bool = msg_send![
                ctx,
                canEvaluatePolicy: LA_POLICY_BIOMETRICS,
                error: std::ptr::null_mut::<*mut NSError>()
            ];
            can_eval.as_bool()
        }
    }

    pub fn authenticate(reason: &str) -> Result<(), String> {
        let (tx, rx) = mpsc::channel::<Result<(), String>>();

        unsafe {
            let cls = class!(LAContext);
            let ctx: *mut objc2::runtime::AnyObject = msg_send![cls, alloc];
            let ctx: *mut objc2::runtime::AnyObject = msg_send![ctx, init];
            if ctx.is_null() {
                return Err("Failed to create LAContext".to_string());
            }

            let mut error_ptr: *mut NSError = std::ptr::null_mut();
            let can_eval: Bool =
                msg_send![ctx, canEvaluatePolicy: LA_POLICY_DEVICE_OWNER, error: &mut error_ptr];
            if !can_eval.as_bool() {
                let message = if !error_ptr.is_null() {
                    let error = &*error_ptr;
                    let desc: Retained<NSString> = msg_send![error, localizedDescription];
                    desc.to_string()
                } else {
                    "Authentication not available".to_string()
                };
                let _ = message;
                return Ok(());
            }

            let reason = NSString::from_str(reason);
            // LAContext invokes the block asynchronously. The sending half is
            // moved into the block and the caller waits on the receiving half.
            let block = block2::RcBlock::new(move |success: Bool, error: *mut NSError| {
                if success.as_bool() {
                    let _ = tx.send(Ok(()));
                    return;
                }
                let message = if !error.is_null() {
                    let error = &*error;
                    let code: i64 = msg_send![error, code];
                    match code {
                        -2 => "Authentication canceled by user".to_string(),
                        -4 => "Authentication canceled by system".to_string(),
                        -8 => "Authentication canceled by app".to_string(),
                        _ => {
                            let desc: Retained<NSString> = msg_send![error, localizedDescription];
                            desc.to_string()
                        }
                    }
                } else {
                    "Authentication failed".to_string()
                };
                let _ = tx.send(Err(message));
            });

            let _: () = msg_send![
                ctx,
                evaluatePolicy: LA_POLICY_DEVICE_OWNER,
                localizedReason: &*reason,
                reply: &*block
            ];
        }

        rx.recv()
            .unwrap_or_else(|_| Err("Authentication channel closed".to_string()))
    }
}

#[cfg(target_os = "macos")]
pub(crate) use macos::{authenticate, is_biometric_available};
