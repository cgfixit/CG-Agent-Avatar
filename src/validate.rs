//! Fail-closed input checks before any bytes leave the process.

use thiserror::Error;

pub const MAX_MESSAGE_CHARS: usize = 32768;
pub const MAX_SESSION_ID: usize = 80;
pub const MIN_SESSION_ID: usize = 1;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ValidateError {
    #[error("message is empty")]
    EmptyMessage,
    #[error("message exceeds {MAX_MESSAGE_CHARS} characters")]
    MessageTooLong,
    #[error("session id is empty or invalid")]
    SessionIdInvalid,
}

/// What to tell the user when `message` refuses their draft. Only a too-long
/// draft needs saying: an empty one is a plain no-op, and the draft itself is
/// never echoed.
pub fn rejection_notice(err: &ValidateError) -> Option<&'static str> {
    match err {
        ValidateError::MessageTooLong => Some(
            "That message is too long to send (limit: 32,768 characters). \
             Trim it and press Return again; your draft is still in the field.",
        ),
        _ => None,
    }
}

pub fn message(raw: &str) -> Result<&str, ValidateError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ValidateError::EmptyMessage);
    }
    if trimmed.chars().count() > MAX_MESSAGE_CHARS {
        return Err(ValidateError::MessageTooLong);
    }
    if trimmed.bytes().any(|b| b == b'\0') {
        return Err(ValidateError::EmptyMessage);
    }
    Ok(trimmed)
}

pub fn session_id(raw: &str) -> Result<&str, ValidateError> {
    let n = raw.len();
    if !(MIN_SESSION_ID..=MAX_SESSION_ID).contains(&n) {
        return Err(ValidateError::SessionIdInvalid);
    }
    if !raw
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(ValidateError::SessionIdInvalid);
    }
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_objections() {
        assert_eq!(message("  ").unwrap_err(), ValidateError::EmptyMessage);
        assert_eq!(message("").unwrap_err(), ValidateError::EmptyMessage);
        assert_eq!(message("ok").unwrap(), "ok");
        assert_eq!(message("  hi  ").unwrap(), "hi");
        let too = "a".repeat(MAX_MESSAGE_CHARS + 1);
        assert_eq!(message(&too).unwrap_err(), ValidateError::MessageTooLong);
        assert!(message(&"a".repeat(MAX_MESSAGE_CHARS)).is_ok());
        assert_eq!(message("x\0y").unwrap_err(), ValidateError::EmptyMessage);
    }

    #[test]
    fn only_an_oversized_draft_gets_a_notice() {
        assert_eq!(
            MAX_MESSAGE_CHARS, 32_768,
            "notice text hard-codes the limit"
        );
        let notice = rejection_notice(&ValidateError::MessageTooLong).unwrap();
        assert!(notice.contains("32,768"));
        assert_eq!(rejection_notice(&ValidateError::EmptyMessage), None);
        assert_eq!(rejection_notice(&ValidateError::SessionIdInvalid), None);
    }

    #[test]
    fn session_id_objections() {
        assert!(session_id("abc").is_ok());
        assert!(session_id("deadbeef-cafe").is_ok());
        assert_eq!(session_id("").unwrap_err(), ValidateError::SessionIdInvalid);
        assert_eq!(
            session_id("../etc").unwrap_err(),
            ValidateError::SessionIdInvalid
        );
        assert_eq!(
            session_id("s/id").unwrap_err(),
            ValidateError::SessionIdInvalid
        );
        assert_eq!(
            session_id("id with space").unwrap_err(),
            ValidateError::SessionIdInvalid
        );
        assert_eq!(
            session_id(&"a".repeat(MAX_SESSION_ID + 1)).unwrap_err(),
            ValidateError::SessionIdInvalid
        );
        assert_eq!(
            session_id("id\nid").unwrap_err(),
            ValidateError::SessionIdInvalid
        );
    }
}
