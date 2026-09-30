use regex::Regex;
use serde::{Deserialize, Serialize};
use totp_rs::{Algorithm, Builder, Secret, Totp};
use zeroize::Zeroizing;

pub const DEFAULT_TOTP_PROMPT: &str = r"(?i)^\s*(?:mfa\s*code|(?:verification|authentication|authenticator|security)\s+code|one[- ]time\s*(?:code|password)|(?:otp|totp)(?:\s*(?:code|password))?|验证码)\s*[:：]?\s*$";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum TotpAlgorithm {
    #[default]
    Sha1,
    Sha256,
    Sha512,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TotpParameters {
    pub algorithm: TotpAlgorithm,
    pub digits: u8,
    pub period: u64,
}

impl Default for TotpParameters {
    fn default() -> Self {
        Self {
            algorithm: TotpAlgorithm::Sha1,
            digits: 6,
            period: 30,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TotpCredential {
    pub id: String,
    pub name: String,
    pub parameters: TotpParameters,
    pub prompt_pattern: String,
    pub enabled: bool,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub secret_revision: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) secret_reference: String,
}

impl TotpCredential {
    pub fn portable(&self) -> Self {
        Self {
            secret_reference: String::new(),
            ..self.clone()
        }
    }

    pub fn validate(&self) -> Result<(), TotpError> {
        if self.id.is_empty()
            || self.secret_revision.is_empty()
            || self.name.trim().is_empty()
            || self.name.len() > 256
            || !matches!(self.parameters.digits, 6 | 8)
            || !(1..=300).contains(&self.parameters.period)
        {
            return Err(TotpError::InvalidParameters);
        }
        validate_totp_pattern(&self.prompt_pattern)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error, Eq, PartialEq)]
pub enum TotpError {
    #[error("Invalid TOTP secret or otpauth URI")]
    InvalidSecret,
    #[error("Invalid TOTP parameters")]
    InvalidParameters,
    #[error("Invalid TOTP prompt pattern")]
    InvalidPattern,
    #[error("TOTP credential is unavailable")]
    Unavailable,
    #[error("TOTP credential is still used by connections")]
    InUse,
    #[error("Could not save TOTP credential")]
    SaveFailed,
}

// The generator owns and zeroizes decoded secret bytes. Keep it inside the
// authentication attempt; never serialize it or expose library parsing errors.
pub struct TotpGenerator(Totp, TotpParameters);

impl TotpGenerator {
    pub fn parse(input: &str, parameters: TotpParameters) -> Result<Self, TotpError> {
        if input.len() > 4096 {
            return Err(TotpError::InvalidSecret);
        }
        let input = input.trim();
        let totp = if input.starts_with("otpauth://") {
            Totp::from_url_unchecked(input).map_err(|_| TotpError::InvalidSecret)?
        } else {
            let mut normalized = Zeroizing::new(
                input
                    .chars()
                    .filter(|c| !c.is_ascii_whitespace())
                    .collect::<String>(),
            );
            normalized.make_ascii_uppercase();
            let secret = Secret::try_from_base32(normalized.as_str())
                .map_err(|_| TotpError::InvalidSecret)?;
            if secret.as_ref().is_empty() {
                return Err(TotpError::InvalidSecret);
            }
            Builder::new()
                .with_secret(secret)
                .with_algorithm(match parameters.algorithm {
                    TotpAlgorithm::Sha1 => Algorithm::SHA1,
                    TotpAlgorithm::Sha256 => Algorithm::SHA256,
                    TotpAlgorithm::Sha512 => Algorithm::SHA512,
                })
                .with_digits(parameters.digits)
                .with_step_duration(parameters.period)
                .build_noncompliant()
        };
        if totp.secret().as_ref().is_empty() {
            return Err(TotpError::InvalidSecret);
        }
        if !matches!(totp.digits(), 6 | 8) || !(1..=300).contains(&totp.step()) {
            return Err(TotpError::InvalidParameters);
        }
        let algorithm = match totp.algorithm() {
            Algorithm::SHA1 => TotpAlgorithm::Sha1,
            Algorithm::SHA256 => TotpAlgorithm::Sha256,
            Algorithm::SHA512 => TotpAlgorithm::Sha512,
            _ => return Err(TotpError::InvalidParameters),
        };
        let parameters = TotpParameters {
            algorithm,
            digits: totp.digits(),
            period: totp.step(),
        };
        Ok(Self(totp, parameters))
    }

    pub fn parameters(&self) -> TotpParameters {
        self.1
    }

    pub(crate) fn encoded_secret(&self) -> Zeroizing<String> {
        Zeroizing::new(self.0.secret().to_base32())
    }

    pub fn generate(&self, unix_seconds: u64) -> Zeroizing<String> {
        Zeroizing::new(self.0.generate(unix_seconds).to_string())
    }
}

pub fn validate_totp_pattern(pattern: &str) -> Result<Regex, TotpError> {
    if pattern.is_empty() || pattern.len() > 512 {
        return Err(TotpError::InvalidPattern);
    }
    let regex = Regex::new(pattern).map_err(|_| TotpError::InvalidPattern)?;
    if regex.is_match("") {
        return Err(TotpError::InvalidPattern);
    }
    Ok(regex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_rfc6238_vectors_and_parses_uri_parameters() {
        let input = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
        let generator = TotpGenerator::parse(
            input,
            TotpParameters {
                digits: 8,
                ..Default::default()
            },
        )
        .unwrap();
        for (time, expected) in [
            (59, "94287082"),
            (1111111109, "07081804"),
            (1111111111, "14050471"),
            (1234567890, "89005924"),
            (2000000000, "69279037"),
            (20000000000, "65353130"),
        ] {
            assert_eq!(generator.generate(time).as_str(), expected);
        }
        let uri = "otpauth://totp/Test?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZA&algorithm=SHA256&digits=8&period=30";
        let generator = TotpGenerator::parse(uri, Default::default()).unwrap();
        assert_eq!(generator.generate(59).as_str(), "46119246");
        assert_eq!(generator.parameters().algorithm, TotpAlgorithm::Sha256);
    }

    #[test]
    fn rejects_invalid_input_without_echoing_secrets_and_matches_only_otp_prompts() {
        for input in [
            "secret-value!",
            "otpauth://hotp/Test?secret=private",
            "otpauth://totp/Test?secret=private&digits=0",
            "",
        ] {
            let error = TotpGenerator::parse(input, Default::default())
                .err()
                .unwrap();
            assert!(!error.to_string().contains("private"));
            assert!(!error.to_string().contains("secret-value"));
        }
        let pattern = validate_totp_pattern(DEFAULT_TOTP_PROMPT).unwrap();
        for prompt in ["MFA CODE: ", "Verification code:", "OTP:", "验证码："] {
            assert!(pattern.is_match(prompt));
        }
        for prompt in [
            "Password:",
            "PIN:",
            "Authentication:",
            "Security:",
            "Press enter for MFA CODE help",
            "",
        ] {
            assert!(!pattern.is_match(prompt));
        }
    }
}
