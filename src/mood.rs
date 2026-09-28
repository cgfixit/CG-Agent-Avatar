//! Mood from harness status + in-flight chat. No ML.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mood {
    Asleep,
    Idle,
    Thinking,
    Talking,
    Sick,
}

#[derive(Debug, Clone)]
pub struct MoodInput {
    pub reachable: bool,
    pub http_ok: bool,
    pub api_key_optional: bool,
    pub model: String,
    pub provider: String,
    pub chat_in_flight: bool,
    pub talking_until: bool,
}

/// What the last network check of the selected backend found. Polled far
/// less often than the mood is redrawn, so in-flight chat state still shows
/// immediately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Health {
    Asleep,
    Sick,
    Ready {
        api_key_optional: bool,
        model: String,
        provider: String,
    },
}

impl Health {
    pub fn input(&self, chat_in_flight: bool, talking_until: bool) -> MoodInput {
        let (reachable, ready) = match self {
            Health::Asleep => (false, None),
            Health::Sick => (true, None),
            Health::Ready {
                api_key_optional,
                model,
                provider,
            } => (true, Some((*api_key_optional, model, provider))),
        };
        MoodInput {
            reachable,
            http_ok: ready.is_some(),
            api_key_optional: ready.is_none_or(|r| r.0),
            model: ready.map(|r| r.1.clone()).unwrap_or_default(),
            provider: ready.map(|r| r.2.clone()).unwrap_or_default(),
            chat_in_flight: ready.is_some() && chat_in_flight,
            talking_until: ready.is_some() && talking_until,
        }
    }
}

pub fn mood(input: MoodInput) -> Mood {
    if !input.reachable {
        return Mood::Asleep;
    }
    if !input.http_ok {
        return Mood::Sick;
    }
    if !input.api_key_optional {
        return Mood::Sick;
    }
    if input.model.trim().is_empty() || input.provider.trim().is_empty() {
        return Mood::Sick;
    }
    if input.chat_in_flight {
        return Mood::Thinking;
    }
    if input.talking_until {
        return Mood::Talking;
    }
    Mood::Idle
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> MoodInput {
        MoodInput {
            reachable: true,
            http_ok: true,
            api_key_optional: true,
            model: "qwen".into(),
            provider: "ollama".into(),
            chat_in_flight: false,
            talking_until: false,
        }
    }

    #[test]
    fn asleep_when_unreachable() {
        let mut i = base();
        i.reachable = false;
        assert_eq!(mood(i), Mood::Asleep);
    }

    #[test]
    fn thinking_beats_idle() {
        let mut i = base();
        i.chat_in_flight = true;
        assert_eq!(mood(i), Mood::Thinking);
    }

    #[test]
    fn sick_when_key_required_or_empty_model() {
        let mut i = base();
        i.api_key_optional = false;
        assert_eq!(mood(i.clone()), Mood::Sick);
        i.api_key_optional = true;
        i.model.clear();
        assert_eq!(mood(i), Mood::Sick);
    }

    #[test]
    fn health_maps_onto_the_existing_moods() {
        let ready = Health::Ready {
            api_key_optional: true,
            model: "m".into(),
            provider: "p".into(),
        };
        assert_eq!(mood(Health::Asleep.input(true, true)), Mood::Asleep);
        assert_eq!(mood(Health::Sick.input(true, true)), Mood::Sick);
        assert_eq!(mood(ready.input(false, false)), Mood::Idle);
        assert_eq!(mood(ready.input(true, false)), Mood::Thinking);
        assert_eq!(mood(ready.input(false, true)), Mood::Talking);
    }

    #[test]
    fn talking_after_reply() {
        let mut i = base();
        i.talking_until = true;
        assert_eq!(mood(i), Mood::Talking);
    }
}
