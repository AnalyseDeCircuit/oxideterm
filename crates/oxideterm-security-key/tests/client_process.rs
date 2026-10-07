use oxideterm_security_key::*;
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};
use zeroize::Zeroizing;

struct Interaction {
    events: Mutex<Vec<&'static str>>,
    touched: tokio::sync::Notify,
}
impl SecurityKeyInteraction for Interaction {
    fn pin(
        &self,
        retry: bool,
    ) -> Pin<Box<dyn Future<Output = Result<Zeroizing<String>, SecurityKeyError>> + Send + '_>>
    {
        self.events
            .lock()
            .unwrap()
            .push(if retry { "pinRetry" } else { "pin" });
        Box::pin(async move {
            Ok(Zeroizing::new(
                if retry { "123456" } else { "111111" }.into(),
            ))
        })
    }
    fn touch(&self) -> Pin<Box<dyn Future<Output = Result<(), SecurityKeyError>> + Send + '_>> {
        self.events.lock().unwrap().push("touch");
        self.touched.notify_one();
        Box::pin(std::future::pending())
    }
}
fn interaction() -> Arc<Interaction> {
    Arc::new(Interaction {
        events: Mutex::new(Vec::new()),
        touched: tokio::sync::Notify::new(),
    })
}
fn request(handle: &str) -> SecurityKeyRequest {
    SecurityKeyRequest::Sign {
        algorithm: "sk-ssh-ed25519@openssh.com".into(),
        application: "ssh:fixture".into(),
        flags: 5,
        key_handle: Zeroizing::new(handle.as_bytes().to_vec()),
        challenge: b"SSH authentication challenge".to_vec(),
    }
}

fn fixture() {
    assert!(
        std::env::var_os("FIDO_TEST_SECRET").is_none(),
        "application credential crossed the signing boundary"
    );
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();
    write_message(
        &mut output,
        &SecurityKeyResponse::Ready {
            protocol_version: 1,
        },
    )
    .unwrap();
    let SecurityKeyRequest::Sign {
        key_handle,
        challenge,
        application,
        ..
    } = read_message(&mut input).unwrap()
    else {
        panic!("expected sign");
    };
    assert_eq!(challenge, b"SSH authentication challenge");
    assert_eq!(application, "ssh:fixture");
    match key_handle.as_slice() {
        b"wait" => {
            write_message(&mut output, &SecurityKeyResponse::Touch).unwrap();
            loop {
                std::thread::park();
            }
        }
        b"oversized" => {
            use std::io::Write;
            output.write_all(&65537_u32.to_be_bytes()).unwrap();
            output.flush().unwrap();
        }
        b"pin" => {
            for (retry, expected) in [(false, "111111"), (true, "123456")] {
                write_message(&mut output, &SecurityKeyResponse::PinRequired { retry }).unwrap();
                let SecurityKeyRequest::Pin { value } = read_message(&mut input).unwrap() else {
                    panic!("expected PIN");
                };
                assert_eq!(value.as_str(), expected);
            }
            write_message(&mut output, &SecurityKeyResponse::Touch).unwrap();
            write_message(
                &mut output,
                &SecurityKeyResponse::Signature {
                    signature: vec![7; 64],
                    flags: 5,
                    counter: 42,
                },
            )
            .unwrap();
        }
        _ => panic!("unexpected fixture credential"),
    }
}

async fn checks() {
    let executable = std::env::current_exe().unwrap();
    let provider = SecurityKeyProvider::new(executable.clone());
    let prompts = interaction();
    let answer = provider
        .sign(request("pin"), prompts.clone())
        .await
        .unwrap();
    match answer {
        SecurityKeyResponse::Signature {
            signature,
            flags,
            counter,
        } => {
            assert_eq!(signature, vec![7; 64]);
            assert_eq!((flags, counter), (5, 42));
        }
        _ => panic!("expected signature"),
    }
    assert_eq!(
        *prompts.events.lock().unwrap(),
        ["pin", "pinRetry", "touch"]
    );
    assert!(matches!(
        provider.sign(request("oversized"), interaction()).await,
        Err(SecurityKeyError::Incompatible)
    ));
    for abort_caller in [false, true] {
        let provider = SecurityKeyProvider::new(executable.clone());
        let prompts = interaction();
        let task = tokio::spawn({
            let provider = provider.clone();
            let prompts = prompts.clone();
            async move { provider.sign(request("wait"), prompts).await }
        });
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            prompts.touched.notified(),
        )
        .await
        .unwrap();
        if abort_caller {
            task.abort();
        }
        let worker = provider.retire();
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            tokio::task::spawn_blocking(move || worker.join().unwrap()),
        )
        .await
        .unwrap()
        .unwrap();
        if abort_caller {
            assert!(matches!(task.await, Err(error) if error.is_cancelled()));
        } else {
            assert!(matches!(
                task.await.unwrap(),
                Err(SecurityKeyError::Cancelled)
            ));
        }
        assert!(matches!(
            provider.sign(request("pin"), interaction()).await,
            Err(SecurityKeyError::Unavailable)
        ));
    }
    println!(
        "Verified real stdio framing, PIN retry, oversized-frame rejection, caller cancellation and provider retirement."
    );
}

fn main() {
    // The fixture is confined to this test executable; it is never included
    // in the plugin or enabled by a switch in a production authentication path.
    if std::env::args().any(|arg| arg == "--stdio") {
        fixture();
        return;
    }
    // This standalone harness sets its synthetic credential before starting threads.
    unsafe {
        std::env::set_var("FIDO_TEST_SECRET", "synthetic-application-token");
    }
    let arguments = std::env::args().collect::<Vec<_>>();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    if let Some(index) = arguments.iter().position(|arg| arg == "--provider") {
        let executable = std::path::PathBuf::from(&arguments[index + 1]);
        runtime.block_on(async {
            let provider = SecurityKeyProvider::new(executable);
            assert!(matches!(
                provider
                    .sign(request("unregistered-fixture"), interaction())
                    .await,
                Err(SecurityKeyError::NoDevice)
            ));
            println!(
                "Verified the host client against the real FIDO provider with no physical device."
            );
        });
    } else {
        runtime.block_on(checks());
    }
}
