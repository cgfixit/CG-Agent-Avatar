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
    fn talking_after_reply() {
        let mut i = base();
        i.talking_until = true;
        assert_eq!(mood(i), Mood::Talking);
    }
}
