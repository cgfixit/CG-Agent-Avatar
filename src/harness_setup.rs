//! User-driven Harness setup, advanced only by a confirmed status response.

use crate::client::Status;
use crate::discover::Reachable;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Off,
    Waiting,
    Login(Reachable),
    Reset(Reachable),
    Configure(Reachable),
    Ready(Reachable),
}

impl Phase {
    pub fn guidance(self) -> &'static str {
        match self {
            Self::Off => "direct ollama — qwen3.8:27b-mlx on :11434",
            Self::Waiting => "waiting for Harness on loopback…",
            Self::Login(_) => "Harness ready — use Harness Login… in the menu",
            Self::Reset(_) => "bootstrap password must be changed — use Harness Password Reset…",
            Self::Configure(_) => "configure a usable model, provider, and key in Harness",
            Self::Ready(_) => "Harness ready — type below, Return to send",
        }
    }
}

#[derive(Debug)]
pub struct Guide {
    generation: u64,
    phase: Phase,
}

impl Default for Guide {
    fn default() -> Self {
        Self {
            generation: 0,
            phase: Phase::Off,
        }
    }
}

impl Guide {
    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn select(&mut self, harness: bool) {
        self.generation = self.generation.wrapping_add(1);
        self.phase = if harness { Phase::Waiting } else { Phase::Off };
    }

    pub fn current(&self, generation: u64) -> bool {
        self.generation == generation && self.phase != Phase::Off
    }

    pub fn endpoint(&self, generation: u64) -> Option<Reachable> {
        if !self.current(generation) {
            return None;
        }
        match self.phase {
            Phase::Login(endpoint)
            | Phase::Reset(endpoint)
            | Phase::Configure(endpoint)
            | Phase::Ready(endpoint) => Some(endpoint),
            Phase::Off | Phase::Waiting => None,
        }
    }

    pub fn ready_endpoint(&self, generation: u64) -> Option<Reachable> {
        if !self.current(generation) {
            return None;
        }
        match self.phase {
            Phase::Ready(endpoint) => Some(endpoint),
            _ => None,
        }
    }

    pub fn reset_endpoint(&self, generation: u64) -> Option<Reachable> {
        if !self.current(generation) {
            return None;
        }
        match self.phase {
            Phase::Reset(endpoint) => Some(endpoint),
            _ => None,
        }
    }

    /// Return whether the visible guide changed. Late status from an earlier
    /// backend selection cannot advance the new selection.
    pub fn observe(&mut self, generation: u64, confirmed: Option<(Reachable, &Status)>) -> bool {
        if !self.current(generation) {
            return false;
        }
        let next = match confirmed {
            None => Phase::Waiting,
            Some((endpoint, _)) if matches!(self.phase, Phase::Reset(_)) => Phase::Reset(endpoint),
            Some((endpoint, status)) if status.auth_enabled && status.model.is_empty() => {
                Phase::Login(endpoint)
            }
            Some((endpoint, status))
                if status.model.trim().is_empty()
                    || status.provider.trim().is_empty()
                    || !status.api_key_optional =>
            {
                Phase::Configure(endpoint)
            }
            Some((endpoint, _)) => Phase::Ready(endpoint),
        };
        let changed = self.phase != next;
        self.phase = next;
        changed
    }

    pub fn login_result(&mut self, generation: u64, must_change_password: bool) -> bool {
        if !self.current(generation) {
            return false;
        }
        let Some(endpoint) = self.endpoint(generation) else {
            return false;
        };
        self.phase = if must_change_password {
            Phase::Reset(endpoint)
        } else {
            Phase::Waiting
        };
        true
    }

    pub fn password_changed(&mut self, generation: u64) -> bool {
        if self.reset_endpoint(generation).is_none() {
            return false;
        }
        self.phase = Phase::Waiting;
        true
    }

    pub fn auth_required(&mut self, generation: u64, reset: bool) -> bool {
        let Some(endpoint) = self.endpoint(generation) else {
            return false;
        };
        self.phase = if reset {
            Phase::Reset(endpoint)
        } else {
            Phase::Login(endpoint)
        };
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(model: &str, provider: &str, key: bool, auth: bool) -> Status {
        Status {
            model: model.into(),
            provider: provider.into(),
            api_key_optional: key,
            version: "1".into(),
            auth_enabled: auth,
        }
    }

    #[test]
    fn setup_requires_confirmed_status_and_reset_repoll() {
        let mut guide = Guide::default();
        let endpoint = Reachable::Https(51234);
        guide.select(true);
        let epoch = guide.generation();
        assert_eq!(guide.phase(), Phase::Waiting);
        assert_eq!(guide.ready_endpoint(epoch), None);
        guide.observe(epoch, Some((endpoint, &status("", "", false, true))));
        assert_eq!(guide.phase(), Phase::Login(endpoint));
        assert_eq!(guide.ready_endpoint(epoch), None);
        assert!(guide.login_result(epoch, true));
        assert_eq!(guide.phase(), Phase::Reset(endpoint));
        guide.observe(epoch, Some((endpoint, &status("m", "p", true, false))));
        assert_eq!(guide.phase(), Phase::Reset(endpoint));
        assert!(guide.password_changed(epoch));
        assert_eq!(guide.phase(), Phase::Waiting);
        assert_eq!(guide.ready_endpoint(epoch), None);
        guide.observe(epoch, Some((endpoint, &status("m", "p", true, false))));
        assert_eq!(guide.ready_endpoint(epoch), Some(endpoint));
    }

    #[test]
    fn stale_results_and_incomplete_configuration_never_unlock_chat() {
        let mut guide = Guide::default();
        guide.select(true);
        let old = guide.generation();
        guide.select(false);
        guide.select(true);
        let epoch = guide.generation();
        let endpoint = Reachable::Http(8790);
        assert!(!guide.observe(old, Some((endpoint, &status("m", "p", true, false)))));
        assert!(!guide.login_result(old, false));
        assert_eq!(guide.phase(), Phase::Waiting);
        guide.observe(epoch, Some((endpoint, &status("m", "", true, false))));
        assert_eq!(guide.phase(), Phase::Configure(endpoint));
        guide.observe(epoch, Some((endpoint, &status("m", "p", false, false))));
        assert_eq!(guide.ready_endpoint(epoch), None);
        guide.observe(epoch, Some((endpoint, &status("m", "p", true, false))));
        assert_eq!(guide.ready_endpoint(epoch), Some(endpoint));
        guide.observe(epoch, None);
        assert_eq!(guide.ready_endpoint(epoch), None);
    }
}
