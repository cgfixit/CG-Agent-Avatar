//! Stop requests for the chat turn in flight. Each turn the chat worker
//! starts gets a number; **Stop** asks the turn running now to end, so a
//! request can never reach a later turn. The worker reads it each time the
//! backend delivers more of the reply and then drops the connection, which
//! ends generation in Ollama and aborts the turn in Harness.

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct Turns {
    current: AtomicU64,
    stop: AtomicU64,
}

impl Turns {
    /// Numbers a new turn. Call it under the lock that hands the message to
    /// the worker, which **Stop** also takes, so a stop is either seen with
    /// the message still queued or lands on this turn.
    pub fn begin(&self) -> u64 {
        self.current.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Asks the turn running now to stop. Before any turn, it does nothing.
    pub fn request_stop(&self) {
        let current = self.current.load(Ordering::SeqCst);
        if current != 0 {
            self.stop.store(current, Ordering::SeqCst);
        }
    }

    /// Whether the user asked `turn` to stop.
    pub fn stop_requested(&self, turn: u64) -> bool {
        turn != 0 && self.stop.load(Ordering::SeqCst) == turn
    }

    /// Whether the latest turn has a stop request; meaningful only while a
    /// reply is in flight.
    pub fn stopping(&self) -> bool {
        self.stop_requested(self.current.load(Ordering::SeqCst))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn nothing_is_stopped_before_a_stop_request() {
        let turns = Turns::default();
        turns.request_stop();
        assert!(!turns.stopping());
        let first = turns.begin();
        assert!(!turns.stop_requested(first));
        assert!(!turns.stopping());
        assert!(!turns.stop_requested(0));
    }

    #[test]
    fn a_stop_reaches_only_the_turn_running_when_it_was_asked() {
        let turns = Turns::default();
        let first = turns.begin();
        turns.request_stop();
        assert!(turns.stop_requested(first));
        assert!(turns.stopping());
        let second = turns.begin();
        assert!(turns.stop_requested(first));
        assert!(!turns.stop_requested(second));
        assert!(!turns.stopping());
    }

    #[test]
    fn repeated_stops_are_harmless() {
        let turns = Turns::default();
        let turn = turns.begin();
        turns.request_stop();
        turns.request_stop();
        assert!(turns.stop_requested(turn));
    }

    #[test]
    fn the_main_thread_stops_a_worker_turn() {
        let turns = Arc::new(Turns::default());
        let turn = turns.begin();
        let main = Arc::clone(&turns);
        std::thread::spawn(move || main.request_stop())
            .join()
            .unwrap();
        assert!(turns.stop_requested(turn));
    }
}
