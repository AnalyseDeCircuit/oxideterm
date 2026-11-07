// Copyright (C) 2026 jaime.su
// SPDX-License-Identifier: GPL-3.0-only

//! Validators for values that come back from the remote herdr server and
//! therefore cannot be trusted when they are interpolated into a command line.

const HERDR_ID_MAX_LEN: usize = 32;
const HERDR_NAME_MAX_LEN: usize = 128;

/// Herdr ids look like `wA` / `wA:pC` / `wA:tB`; the grammar is stricter than
/// a free label so a hostile id cannot smuggle shell metacharacters.
pub(crate) fn validated_id(id: &str) -> Result<String, String> {
    // Padding is rejected, never trimmed: an action must address exactly the
    // id the panel displayed, and normalizing a server-authored value would
    // let `" wA"` and `"wA"` select the same object from two different rows.
    if id != id.trim()
        || id.is_empty()
        || id.len() > HERDR_ID_MAX_LEN
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-' | '.'))
    {
        return Err("Invalid herdr id.".to_string());
    }
    Ok(id.to_string())
}

/// Session names, workspace labels and pane labels are server-authored free
/// text, so the grammar is only about what a command line can carry: length
/// and control characters. Every other character reaches herdr as data, never
/// as script text, so a smart quote or a pipe in a label is not a reason to
/// refuse the row it came from.
pub(crate) fn validated_name(name: &str) -> Result<String, String> {
    // Rejected rather than trimmed, for the same reason as [`validated_id`].
    if name != name.trim()
        || name.is_empty()
        || name.chars().count() > HERDR_NAME_MAX_LEN
        || name.chars().any(char::is_control)
    {
        return Err("Invalid herdr name.".to_string());
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_accepted_only_exactly_as_sent() {
        assert_eq!(validated_id("wA:tB:p1").unwrap(), "wA:tB:p1");
        for padded in [" wA", "wA ", "\twA", "wA\r", " "] {
            assert!(validated_id(padded).is_err(), "{padded:?}");
        }
        let too_long = "x".repeat(HERDR_ID_MAX_LEN + 1);
        for hostile in ["", "wA; rm", "wA tB", "wA\n", too_long.as_str()] {
            assert!(validated_id(hostile).is_err(), "{hostile:?}");
        }
    }

    #[test]
    fn names_are_accepted_only_exactly_as_sent() {
        assert_eq!(validated_name("work space").unwrap(), "work space");
        // The characters that used to be dangerous in a quoted argument are
        // now just label text: they travel as data.
        assert!(validated_name("it's ’ & | ^ % $(id)").is_ok());
        for padded in [" work", "work ", "  ", "\nwork"] {
            assert!(validated_name(padded).is_err(), "{padded:?}");
        }
        let too_long = "x".repeat(HERDR_NAME_MAX_LEN + 1);
        for hostile in ["", "line\nbreak", "tab\there", too_long.as_str()] {
            assert!(validated_name(hostile).is_err(), "{hostile:?}");
        }
    }
}
