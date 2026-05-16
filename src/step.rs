fn same_label(input: &str, expected: &str) -> bool {
    input.trim().eq_ignore_ascii_case(expected)
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Step {
    pub note: u8,
    pub transpose: Transpose,
    pub accent: Accent,
    pub slide: Slide,
    pub time: Time,
}

impl Step {
    pub const COUNT: usize = 16;

    pub const fn new(
        note: u8,
        transpose: Transpose,
        accent: Accent,
        slide: Slide,
        time: Time,
    ) -> Self {
        Self {
            note,
            transpose,
            accent,
            slide,
            time,
        }
    }
}

impl Default for Step {
    fn default() -> Self {
        Self {
            note: 0,
            transpose: Transpose::Normal,
            accent: Accent::Off,
            slide: Slide::Off,
            time: Time::Normal,
        }
    }
}

#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Transpose {
    Down = 0,
    Normal = 1,
    Up = 2,
}

impl Transpose {
    pub const fn contract_name(self) -> &'static str {
        match self {
            Self::Down => "DOWN",
            Self::Normal => "NORMAL",
            Self::Up => "UP",
        }
    }

    pub const fn pitch_base_offset(self) -> u8 {
        match self {
            Self::Down => 0,
            Self::Normal => 12,
            Self::Up => 24,
        }
    }

    pub fn from_contract(text: &str) -> Result<Self, ()> {
        if same_label(text, "DOWN") {
            Ok(Self::Down)
        } else if same_label(text, "NORMAL") {
            Ok(Self::Normal)
        } else if same_label(text, "UP") {
            Ok(Self::Up)
        } else {
            Err(())
        }
    }

    pub const fn steps_symbol(self) -> u8 {
        match self {
            Self::Down => b'D',
            Self::Normal => b'-',
            Self::Up => b'U',
        }
    }

    pub fn from_steps_symbol(symbol: u8) -> Result<Self, ()> {
        match symbol.to_ascii_uppercase() {
            b'D' => Ok(Self::Down),
            b'-' => Ok(Self::Normal),
            b'U' => Ok(Self::Up),
            _ => Err(()),
        }
    }
}

impl std::convert::TryFrom<u8> for Transpose {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Down),
            1 => Ok(Self::Normal),
            2 => Ok(Self::Up),
            _ => Err(()),
        }
    }
}

macro_rules! binary_step_flag {
    ($name:ident, $symbol:literal) => {
        #[repr(u8)]
        #[derive(Debug, Copy, Clone, PartialEq, Eq)]
        pub enum $name {
            Off = 0,
            On = 1,
        }

        impl $name {
            pub const fn enabled(self) -> bool {
                matches!(self, Self::On)
            }

            pub const fn from_enabled(enabled: bool) -> Self {
                if enabled {
                    Self::On
                } else {
                    Self::Off
                }
            }

            pub const fn steps_symbol(self) -> u8 {
                match self {
                    Self::Off => b'-',
                    Self::On => $symbol,
                }
            }

            pub fn from_steps_symbol(symbol: u8) -> Result<Self, ()> {
                match symbol.to_ascii_uppercase() {
                    b'-' => Ok(Self::Off),
                    $symbol => Ok(Self::On),
                    _ => Err(()),
                }
            }
        }

        impl std::convert::TryFrom<u8> for $name {
            type Error = ();

            fn try_from(value: u8) -> Result<Self, Self::Error> {
                match value {
                    0 => Ok(Self::Off),
                    1 => Ok(Self::On),
                    _ => Err(()),
                }
            }
        }
    };
}

binary_step_flag!(Accent, b'A');
binary_step_flag!(Slide, b'S');

#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Time {
    Tie = 0b00,
    Normal = 0b01,
    TieRest = 0b10,
    Rest = 0b11,
}

impl Time {
    pub const fn contract_name(self) -> &'static str {
        match self {
            Self::Tie => "TIE",
            Self::Normal => "NORMAL",
            Self::TieRest => "TIE_REST",
            Self::Rest => "REST",
        }
    }

    pub fn from_contract(text: &str) -> Result<Self, ()> {
        if same_label(text, "TIE") {
            Ok(Self::Tie)
        } else if same_label(text, "NORMAL") {
            Ok(Self::Normal)
        } else if same_label(text, "TIE_REST") {
            Ok(Self::TieRest)
        } else if same_label(text, "REST") {
            Ok(Self::Rest)
        } else {
            Err(())
        }
    }

    pub const fn steps_token(self) -> &'static str {
        match self {
            Self::Normal => "N",
            Self::Tie => "T",
            Self::Rest => "R",
            Self::TieRest => "TR",
        }
    }

    pub fn from_steps_token(text: &str) -> Result<Self, ()> {
        if same_label(text, "N") {
            Ok(Self::Normal)
        } else if same_label(text, "T") {
            Ok(Self::Tie)
        } else if same_label(text, "R") {
            Ok(Self::Rest)
        } else if same_label(text, "TR") {
            Ok(Self::TieRest)
        } else {
            Err(())
        }
    }
}

impl std::convert::TryFrom<u8> for Time {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0b00 => Ok(Self::Tie),
            0b01 => Ok(Self::Normal),
            0b10 => Ok(Self::TieRest),
            0b11 => Ok(Self::Rest),
            _ => Err(()),
        }
    }
}

impl std::convert::TryFrom<u16> for Time {
    type Error = ();

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        if value > u8::MAX as u16 {
            return Err(());
        }
        Self::try_from(value as u8)
    }
}
