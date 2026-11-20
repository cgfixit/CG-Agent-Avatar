//! User-driven Harness setup, advanced only by a confirmed status response.

use crate::client::Status;
use crate::discover::Reachable;
use crate::display;

/// Most characters of a model name shown in the panel's backend label.
const LABEL_MODEL_CHARS: usize = 24;
const CLOUD_PREFIX: &str = "Harness · cloud · ";
/// Most characters of a model or provider name shown in its tooltip.
const TOOLTIP_NAME_CHARS: usize = 64;

/// What the chat panel says about the confirmed Harness: a short label and a
/// longer tooltip. Names come from the Harness status response, so they are
/// sanitized and clipped like any other backend text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendDetail {
    pub label: String,
    pub tooltip: String,
}

/// Describes the selected Harness model. The cloud marker comes before the
/// model name, so a narrow label loses the name's tail, never the marker.
/// `None` before login, when Harness reports no model.
pub fn describe(status: &Status) -> Option<BackendDetail> {
    let model = display::one_line(&status.model, TOOLTIP_NAME_CHARS);
    if model.is_empty() {
        return None;
    }
    let short = display::one_line(&status.model, LABEL_MODEL_CHARS);
    let provider = display::one_line(&status.provider, TOOLTIP_NAME_CHARS);
    let by = if provider.is_empty() {
        String::new()
    } else {
        format!(" ({provider})")
    };
    Some(if status.model_status.cloud {
        BackendDetail {
            label: format!("{CLOUD_PREFIX}{short}"),
            tooltip: format!(
                "Harness is set to the cloud model {model}{by}. Messages you send leave this Mac."
            ),
        }
    } else {
        BackendDetail {
            label: format!("Harness · {short}"),
            tooltip: format!(
                "Harness is set to the local model {model}{by}. Its web tools, when enabled in Harness, can still go online."
            ),
        }
    })
}

/// The backend label and tooltip for the chat panel. While a reply is pending
/// the model name gives way to the activity, but the cloud marker stays.
pub fn panel_label(
    harness: bool,
    detail: Option<&BackendDetail>,
    in_flight: bool,
) -> (String, String) {
    let (label, tooltip) = match (harness, detail) {
        (false, _) => (
            "Direct Ollama".to_string(),
            "Direct Ollama on this Mac. A web lookup you ask for goes out through Ollama."
                .to_string(),
        ),
        (true, None) => (
            "Harness".to_string(),
            "Harness has not confirmed a model yet.".to_string(),
        ),
        (true, Some(detail)) => (detail.label.clone(), detail.tooltip.clone()),
    };
    if !in_flight {
        return (label, tooltip);
    }
    let cloud = detail.is_some_and(|d| d.label.starts_with(CLOUD_PREFIX));
    let busy = match (harness, cloud) {
        (false, _) => "Direct Ollama · Thinking…",
        (true, true) => "Harness · cloud · Thinking…",
        (true, false) => "Harness · Thinking…",
    };
    (busy.to_string(), tooltip)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Off,
    Waiting,
    Login(Reachable),
    Reset(Reachable),
    Configure(Reachable),
    /// A cloud model is selected and its provider key is missing or cloud
    /// chat is off: Harness would refuse the turn.
    CloudSetup(Reachable),
    /// A cloud model is selected and this account has not approved sending
    /// chats to its provider.
    CloudConsent(Reachable),
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
            Self::CloudSetup(_) => {
                "Harness has a cloud model selected that it cannot use (key missing or cloud chat off) — fix that or pick a local model in Harness"
            }
            Self::CloudConsent(_) => {
                "Harness has a cloud model selected — approve cloud chat in Harness, or pick a local model there"
            }
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
            | Phase::CloudSetup(endpoint)
            | Phase::CloudConsent(endpoint)
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
            // Harness reports `api_key_optional: true` unconditionally now, so
            // a cloud selection's readiness is read from `model_status`.
            Some((endpoint, status))
                if status.model_status.disabled || status.model_status.key_missing =>
            {
                Phase::CloudSetup(endpoint)
            }
            Some((endpoint, status)) if status.model_status.consent_required => {
                Phase::CloudConsent(endpoint)
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
    use crate::client::ModelStatus;

    fn status(model: &str, provider: &str, key: bool, auth: bool) -> Status {
        Status {
            model: model.into(),
            provider: provider.into(),
            api_key_optional: key,
            version: "1".into(),
            auth_enabled: auth,
            model_status: ModelStatus::default(),
        }
    }

    fn cloud(disabled: bool, key_missing: bool, consent_required: bool) -> Status {
        Status {
            model_status: ModelStatus {
                cloud: true,
                disabled,
                key_missing,
                consent_required,
            },
            ..status("claude-opus-5-5", "claude", true, false)
        }
    }

    #[test]
    fn cloud_selection_is_ready_only_with_key_and_consent() {
        let mut guide = Guide::default();
        guide.select(true);
        let epoch = guide.generation();
        let endpoint = Reachable::Https(8790);
        for (status, expected) in [
            (cloud(false, true, true), Phase::CloudSetup(endpoint)),
            (cloud(true, false, false), Phase::CloudSetup(endpoint)),
            (cloud(false, false, true), Phase::CloudConsent(endpoint)),
            (cloud(false, false, false), Phase::Ready(endpoint)),
        ] {
            guide.observe(epoch, Some((endpoint, &status)));
            println!(
                "{:?} -> {:?}: {}",
                status.model_status,
                guide.phase(),
                guide.phase().guidance()
            );
            assert_eq!(guide.phase(), expected);
            assert_eq!(
                guide.ready_endpoint(epoch).is_some(),
                expected == Phase::Ready(endpoint)
            );
            // Login and password reset still reach the confirmed endpoint.
            assert_eq!(guide.endpoint(epoch), Some(endpoint));
        }
    }

    #[test]
    fn label_marks_cloud_before_the_model_and_sanitizes_names() {
        assert_eq!(describe(&status("", "", true, true)), None);
        let local = describe(&status("qwen3.8:27b", "ollama", true, false)).unwrap();
        assert_eq!(local.label, "Harness · qwen3.8:27b");
        assert!(local.tooltip.contains("local model qwen3.8:27b (ollama)"));
        let remote = describe(&cloud(false, false, false)).unwrap();
        assert_eq!(remote.label, "Harness · cloud · claude-opus-5-5");
        assert!(remote.tooltip.contains("leave this Mac"));
        let hostile = describe(&Status {
            model: format!("\u{1b}[31mevil\nname\u{202e}{}", "x".repeat(200)),
            ..cloud(false, false, false)
        })
        .unwrap();
        println!(
            "{}\n{}\n{}\n{}",
            local.label, local.tooltip, remote.label, hostile.label
        );
        assert_eq!(
            panel_label(false, None, true).0,
            "Direct Ollama · Thinking…"
        );
        assert_eq!(panel_label(true, None, false).0, "Harness");
        assert_eq!(
            panel_label(true, Some(&local), true).0,
            "Harness · Thinking…"
        );
        assert_eq!(panel_label(true, Some(&remote), false).0, remote.label);
        // The cloud marker outlives the model name while a reply is pending.
        assert_eq!(
            panel_label(true, Some(&remote), true).0,
            "Harness · cloud · Thinking…"
        );
        assert!(hostile.label.starts_with("Harness · cloud · evil name"));
        assert!(hostile.label.chars().count() <= "Harness · cloud · ".chars().count() + 25);
        assert!(!hostile.tooltip.contains(['\u{1b}', '\n', '\u{202e}']));
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
