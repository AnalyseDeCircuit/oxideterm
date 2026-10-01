use super::*;
use oxideterm_connections::{TotpBinding, totp::validate_totp_pattern};
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) struct TotpPromptHandler<'a> {
    pub binding: TotpBinding,
    pub manual: Option<&'a dyn SshPromptHandler>,
    pub submitted: AtomicBool,
}

pub(super) async fn matches_bound_prompt(binding: Option<&TotpBinding>, prompt: &str) -> bool {
    let Some(binding) = binding.cloned() else {
        return false;
    };
    let Ok(Ok(credential)) = tokio::task::spawn_blocking(move || binding.metadata()).await else {
        return false;
    };
    validate_totp_pattern(&credential.prompt_pattern).is_ok_and(|pattern| pattern.is_match(prompt))
}

fn autofill_prompt_index(
    request: &KeyboardInteractivePromptRequest,
    pattern: &str,
) -> Option<usize> {
    let pattern = validate_totp_pattern(pattern).ok()?;
    // Echo controls input display, not its purpose. JumpServer requests visible OTP input.
    let mut matches = request
        .prompts
        .iter()
        .enumerate()
        .filter(|(_, p)| pattern.is_match(&p.prompt));
    let (index, _) = matches.next()?;
    matches.next().is_none().then_some(index)
}

impl SshPromptHandler for TotpPromptHandler<'_> {
    fn keyboard_interactive(
        &self,
        request: KeyboardInteractivePromptRequest,
    ) -> Pin<
        Box<dyn Future<Output = Result<KeyboardInteractiveResponses, SshPromptError>> + Send + '_>,
    > {
        Box::pin(async move {
            let manual = |request| async move {
                match self.manual {
                    Some(handler) => handler.keyboard_interactive(request).await,
                    None => Err(SshPromptError::Failed(
                        "SSH authentication requires manual input".into(),
                    )),
                }
            };
            if self.submitted.load(Ordering::Acquire) {
                return manual(request).await;
            }
            let binding = self.binding.clone();
            let metadata = tokio::task::spawn_blocking(move || binding.metadata()).await;
            let Ok(Ok(credential)) = metadata else {
                return manual(request).await;
            };
            let Some(index) = autofill_prompt_index(&request, &credential.prompt_pattern) else {
                return manual(request).await;
            };
            let mut remaining = request.clone();
            remaining.prompts.remove(index);
            let mut answers = if remaining.prompts.is_empty() {
                Zeroizing::new(Vec::new())
            } else {
                manual(remaining).await?
            };
            if answers.len() + 1 != request.prompts.len() {
                return Err(SshPromptError::Failed(
                    "SSH authentication response count mismatch".into(),
                ));
            }
            // Generate after manual answers arrive. A seed is never retained while
            // waiting for a dialog, and each physical authentication gets one auto attempt.
            let binding = self.binding.clone();
            let generated = tokio::task::spawn_blocking(move || {
                let (current, generator) = binding.resolve()?;
                if current != credential {
                    return Err(oxideterm_connections::totp::TotpError::Unavailable);
                }
                let time = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|_| oxideterm_connections::totp::TotpError::Unavailable)?
                    .as_secs();
                Ok(generator.generate(time))
            })
            .await;
            let Ok(Ok(code)) = generated else {
                return manual(request).await;
            };
            answers.insert(index, code.to_string());
            self.submitted.store(true, Ordering::Release);
            Ok(answers)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxideterm_connections::totp::DEFAULT_TOTP_PROMPT;

    #[test]
    fn totp_prompt_selection_requires_one_match_regardless_of_echo() {
        let mut request = KeyboardInteractivePromptRequest {
            flow_id: "auth-flow".into(),
            name: "Authentication".into(),
            instructions: String::new(),
            chained: true,
            prompts: vec![
                KeyboardInteractivePrompt {
                    prompt: "Password:".into(),
                    echo: false,
                },
                KeyboardInteractivePrompt {
                    prompt: "MFA CODE:".into(),
                    echo: false,
                },
                KeyboardInteractivePrompt {
                    prompt: "Account:".into(),
                    echo: true,
                },
            ],
        };
        assert_eq!(
            autofill_prompt_index(&request, DEFAULT_TOTP_PROMPT),
            Some(1)
        );
        request.prompts[1].echo = true;
        for pattern in [DEFAULT_TOTP_PROMPT, r".*OTP.*"] {
            request.prompts[1].prompt = "[OTP Code]: ".into();
            assert_eq!(autofill_prompt_index(&request, pattern), Some(1));
        }
        request.prompts.push(KeyboardInteractivePrompt {
            prompt: "OTP:".into(),
            echo: false,
        });
        assert_eq!(autofill_prompt_index(&request, DEFAULT_TOTP_PROMPT), None);
        request.prompts.truncate(1);
        assert_eq!(autofill_prompt_index(&request, DEFAULT_TOTP_PROMPT), None);
        request.prompts[0].prompt = "Enter your company token: ".into();
        assert_eq!(
            autofill_prompt_index(&request, r"^Enter your company token: ?$"),
            Some(0)
        );
    }
}
