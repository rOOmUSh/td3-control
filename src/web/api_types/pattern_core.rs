use std::borrow::Cow;

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

use crate::error::Td3Error;
use crate::formats;
use crate::pattern::Pattern;
use crate::step;

// ---------------------------------------------------------------------------
// Pattern
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct PatternRequest {
    pub patgroup: u8,
    pub pattern: u8,
    pub side: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WebNote {
    #[default]
    C,
    CSharp,
    D,
    DSharp,
    E,
    F,
    FSharp,
    G,
    GSharp,
    A,
    ASharp,
    B,
    CHigh,
    Unknown,
}

impl WebNote {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::C => "C",
            Self::CSharp => "C#",
            Self::D => "D",
            Self::DSharp => "D#",
            Self::E => "E",
            Self::F => "F",
            Self::FSharp => "F#",
            Self::G => "G",
            Self::GSharp => "G#",
            Self::A => "A",
            Self::ASharp => "A#",
            Self::B => "B",
            Self::CHigh => "C^",
            Self::Unknown => "??",
        }
    }

    pub fn note_number(self) -> Result<u8, Td3Error> {
        match self {
            Self::C => Ok(0),
            Self::CSharp => Ok(1),
            Self::D => Ok(2),
            Self::DSharp => Ok(3),
            Self::E => Ok(4),
            Self::F => Ok(5),
            Self::FSharp => Ok(6),
            Self::G => Ok(7),
            Self::GSharp => Ok(8),
            Self::A => Ok(9),
            Self::ASharp => Ok(10),
            Self::B => Ok(11),
            Self::CHigh => Ok(12),
            Self::Unknown => Err(Td3Error::FormatError("unknown note: '??'".into())),
        }
    }

    pub const fn from_note_number(note: u8) -> Self {
        match note {
            0 => Self::C,
            1 => Self::CSharp,
            2 => Self::D,
            3 => Self::DSharp,
            4 => Self::E,
            5 => Self::F,
            6 => Self::FSharp,
            7 => Self::G,
            8 => Self::GSharp,
            9 => Self::A,
            10 => Self::ASharp,
            11 => Self::B,
            12 => Self::CHigh,
            _ => Self::Unknown,
        }
    }

    pub fn from_wire(value: &str) -> Result<Self, Td3Error> {
        formats::parse_note_name(value).map(Self::from_note_number)
    }
}

impl Serialize for WebNote {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for WebNote {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Cow::<str>::deserialize(deserializer)?;
        Self::from_wire(&value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WebTranspose {
    Down,
    #[default]
    Normal,
    Up,
}

impl WebTranspose {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Down => "DOWN",
            Self::Normal => "NORMAL",
            Self::Up => "UP",
        }
    }

    pub const fn to_step_transpose(self) -> step::Transpose {
        match self {
            Self::Down => step::Transpose::Down,
            Self::Normal => step::Transpose::Normal,
            Self::Up => step::Transpose::Up,
        }
    }

    pub const fn from_step_transpose(value: step::Transpose) -> Self {
        match value {
            step::Transpose::Down => Self::Down,
            step::Transpose::Normal => Self::Normal,
            step::Transpose::Up => Self::Up,
        }
    }

    pub fn from_wire(value: &str) -> Result<Self, Td3Error> {
        step::Transpose::from_contract(value)
            .map(Self::from_step_transpose)
            .map_err(|_| Td3Error::FormatError(format!("invalid transpose: '{}'", value)))
    }
}

impl Serialize for WebTranspose {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for WebTranspose {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Cow::<str>::deserialize(deserializer)?;
        Self::from_wire(&value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WebTime {
    #[default]
    Normal,
    Tie,
    Rest,
    TieRest,
}

impl WebTime {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Tie => "TIE",
            Self::Rest => "REST",
            Self::TieRest => "TIE_REST",
        }
    }

    pub const fn to_step_time(self) -> step::Time {
        match self {
            Self::Normal => step::Time::Normal,
            Self::Tie => step::Time::Tie,
            Self::Rest => step::Time::Rest,
            Self::TieRest => step::Time::TieRest,
        }
    }

    pub const fn from_step_time(value: step::Time) -> Self {
        match value {
            step::Time::Normal => Self::Normal,
            step::Time::Tie => Self::Tie,
            step::Time::Rest => Self::Rest,
            step::Time::TieRest => Self::TieRest,
        }
    }

    pub fn from_wire(value: &str) -> Result<Self, Td3Error> {
        step::Time::from_contract(value)
            .map(Self::from_step_time)
            .map_err(|_| Td3Error::FormatError(format!("invalid time: '{}'", value)))
    }
}

impl Serialize for WebTime {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for WebTime {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Cow::<str>::deserialize(deserializer)?;
        Self::from_wire(&value).map_err(de::Error::custom)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WebStep {
    pub note: WebNote,
    pub transpose: WebTranspose,
    pub accent: bool,
    pub slide: bool,
    pub time: WebTime,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WebPattern {
    pub active_steps: u8,
    pub triplet: bool,
    pub steps: [WebStep; step::Step::COUNT],
}

impl WebStep {
    pub fn from_step(step_data: &step::Step) -> Self {
        Self {
            note: WebNote::from_note_number(step_data.note),
            transpose: WebTranspose::from_step_transpose(step_data.transpose),
            accent: step_data.accent.enabled(),
            slide: step_data.slide.enabled(),
            time: WebTime::from_step_time(step_data.time),
        }
    }

    pub fn to_step(self) -> Result<step::Step, Td3Error> {
        Ok(step::Step::new(
            self.note.note_number()?,
            self.transpose.to_step_transpose(),
            step::Accent::from_enabled(self.accent),
            step::Slide::from_enabled(self.slide),
            self.time.to_step_time(),
        ))
    }
}

impl WebPattern {
    pub fn from_pattern(pattern: &Pattern) -> Self {
        let mut steps = [WebStep::default(); step::Step::COUNT];
        for (idx, step_data) in pattern.step.iter().enumerate() {
            steps[idx] = WebStep::from_step(step_data);
        }
        Self {
            active_steps: pattern.active_steps,
            triplet: pattern.triplet,
            steps,
        }
    }

    pub fn to_pattern(&self) -> Result<Pattern, Td3Error> {
        let mut steps: [step::Step; 16] = Default::default();
        for (idx, web_step) in self.steps.iter().enumerate() {
            steps[idx] = web_step.to_step()?;
        }

        Pattern::new(self.triplet, self.active_steps, steps)
    }
}

#[derive(Serialize)]
pub struct PatternLoadResponse {
    pub address: String,
    pub pattern: WebPattern,
}

#[derive(Deserialize)]
pub struct PatternSaveRequest {
    pub patgroup: u8,
    pub pattern: u8,
    pub side: String,
    pub data: WebPattern,
}

#[derive(Serialize, Deserialize)]
pub struct PatternSaveResponse {
    pub address: String,
    pub saved: bool,
}
