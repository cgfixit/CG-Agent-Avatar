//! Portable preferences; AppKit owns persistence and controls.

use serde::{Deserialize, Serialize};

use crate::validate::{self, ValidateError};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum OutputStyle {
    #[default]
    Default,
    Concise,
    Detailed,
    Custom,
}

impl OutputStyle {
    pub const ALL: [Self; 4] = [Self::Default, Self::Concise, Self::Detailed, Self::Custom];

    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Concise => "Concise",
            Self::Detailed => "Detailed",
            Self::Custom => "Custom",
        }
    }

    pub fn system_prompt(self, custom: &str) -> Result<String, ValidateError> {
        let instruction = match self {
            Self::Default => return Ok(crate::ollama::SYSTEM_PROMPT.into()),
            Self::Concise => "Use one to three short sentences. Put the answer first; omit background unless requested.",
            Self::Detailed => "Give a thorough explanation with concrete examples when useful. Use short paragraphs or plain-text bullets and stay within the display limit.",
            Self::Custom => validate::output_style(custom)?,
        };
        Ok(format!(
            "{}\n\n## Preferred output style\nApply these presentation preferences while keeping the capability, truthfulness, and plain-text rules above:\n{instruction}",
            crate::ollama::SYSTEM_PROMPT
        ))
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct Settings {
    pub output_style: OutputStyle,
    pub custom_style: String,
    pub reduce_motion: bool,
    pub all_spaces: bool,
    pub keep_above: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            output_style: OutputStyle::Default,
            custom_style: String::new(),
            reduce_motion: false,
            all_spaces: true,
            keep_above: true,
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), ValidateError> {
        if self.output_style == OutputStyle::Custom || !self.custom_style.is_empty() {
            validate::output_style(&self.custom_style)?;
        }
        Ok(())
    }

    pub fn from_json(raw: &str) -> Self {
        serde_json::from_str::<Self>(raw)
            .ok()
            .filter(|settings| settings.validate().is_ok())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_round_trip_and_reject_invalid_saved_input() {
        let settings = Settings {
            output_style: OutputStyle::Custom,
            custom_style: "Use three plain-text bullets.".into(),
            reduce_motion: true,
            all_spaces: false,
            keep_above: false,
        };
        assert_eq!(
            Settings::from_json(&serde_json::to_string(&settings).unwrap()),
            settings
        );
        for invalid in [
            "not json",
            r#"{"output_style":"Unknown"}"#,
            r#"{"output_style":"Custom"}"#,
        ] {
            assert_eq!(Settings::from_json(invalid), Settings::default());
        }
        assert!(Settings::from_json(r#"{"reduce_motion":true}"#).reduce_motion);
    }

    #[test]
    fn styles_preserve_the_base_prompt_and_default_behavior() {
        assert_eq!(
            OutputStyle::Default.system_prompt("").unwrap(),
            crate::ollama::SYSTEM_PROMPT
        );
        for style in OutputStyle::ALL.into_iter().skip(1) {
            let prompt = style
                .system_prompt("Use three plain-text bullets.")
                .unwrap();
            assert!(prompt.starts_with(crate::ollama::SYSTEM_PROMPT));
            assert!(prompt.contains("Preferred output style"));
        }
        assert!(OutputStyle::Custom.system_prompt("").is_err());
    }
}
