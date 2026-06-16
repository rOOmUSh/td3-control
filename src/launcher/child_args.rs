//! Child process argument construction for launcher-started control sessions.

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LauncherMidiChoice {
    #[default]
    EnvDefault,
    ExactPair {
        input: String,
        output: String,
    },
}

impl LauncherMidiChoice {
    pub fn exact_pair(input: impl Into<String>, output: impl Into<String>) -> Self {
        Self::ExactPair {
            input: input.into(),
            output: output.into(),
        }
    }
}

pub fn build_control_args(scratch: &str, midi: &LauncherMidiChoice, web_port: u16) -> Vec<String> {
    let mut args = vec![
        "control".to_string(),
        "--scratch-pattern".to_string(),
        scratch.to_string(),
        "--port".to_string(),
        web_port.to_string(),
    ];

    if let LauncherMidiChoice::ExactPair { input, output } = midi {
        args.push("--midi-in".to_string());
        args.push(input.clone());
        args.push("--midi-out".to_string());
        args.push(output.clone());
        args.push("--strict-device-name".to_string());
    }

    args
}
