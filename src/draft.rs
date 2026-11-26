//! The chat field's unanswered message. Send clears the field, so a turn that
//! ends without an answer (a timeout, a broken stream, a busy or rate-limited
//! Harness, a login prompt, a backend switch) hands its message back here and
//! the main thread puts it in the field again: Return then retries it.

use std::sync::{Mutex, PoisonError};

/// The last unanswered message, carried from the chat worker to the main
/// thread. It lives only in memory and only until the next tick takes it.
#[derive(Debug, Default)]
pub struct Unanswered(Mutex<Option<String>>);

impl Unanswered {
    /// Holds `message` for the field, replacing any message not yet restored.
    /// A blank message is never held.
    pub fn hold(&self, message: String) {
        if message.trim().is_empty() {
            return;
        }
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(message);
    }

    /// Takes the held message for the chat field, whose current text `field`
    /// reads; it runs only when a message is held. The message comes back only
    /// while the field is blank: a draft typed during the turn is never
    /// replaced, and the held message is dropped instead. Either way it is
    /// taken once.
    pub fn restore(&self, field: impl FnOnce() -> String) -> Option<String> {
        let held = self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()?;
        field().trim().is_empty().then_some(held)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn unanswered_message_returns_to_a_blank_field_once() {
        let slot = Unanswered::default();
        slot.hold("why is the build red?".into());
        assert_eq!(
            slot.restore(String::new).as_deref(),
            Some("why is the build red?")
        );
        assert_eq!(slot.restore(String::new), None);
    }

    #[test]
    fn whitespace_only_field_counts_as_blank() {
        let slot = Unanswered::default();
        slot.hold("retry me".into());
        assert_eq!(
            slot.restore(|| " \n\t ".into()).as_deref(),
            Some("retry me")
        );
    }

    #[test]
    fn a_new_draft_is_never_overwritten() {
        let slot = Unanswered::default();
        slot.hold("old question".into());
        assert_eq!(slot.restore(|| "new draft".into()), None);
        // Dropped, not deferred: clearing the draft later brings nothing back.
        assert_eq!(slot.restore(String::new), None);
    }

    #[test]
    fn field_is_not_read_when_nothing_is_held() {
        let slot = Unanswered::default();
        assert_eq!(
            slot.restore(|| unreachable!("field read with nothing held")),
            None
        );
    }

    #[test]
    fn blank_messages_are_not_held_and_newer_failures_win() {
        let slot = Unanswered::default();
        slot.hold("   ".into());
        assert_eq!(slot.restore(String::new), None);
        slot.hold("first".into());
        slot.hold("second".into());
        assert_eq!(slot.restore(String::new).as_deref(), Some("second"));
    }

    #[test]
    fn full_length_message_round_trips_unchanged() {
        let message = "é".repeat(crate::validate::MAX_MESSAGE_CHARS);
        let slot = Unanswered::default();
        slot.hold(message.clone());
        let back = slot.restore(String::new).unwrap();
        assert_eq!(back, message);
        assert!(crate::validate::message(&back).is_ok());
    }

    #[test]
    fn worker_thread_hands_off_to_main_thread() {
        let slot = Arc::new(Unanswered::default());
        let worker = Arc::clone(&slot);
        std::thread::spawn(move || worker.hold("from the worker".into()))
            .join()
            .unwrap();
        assert_eq!(
            slot.restore(String::new).as_deref(),
            Some("from the worker")
        );
    }
}
