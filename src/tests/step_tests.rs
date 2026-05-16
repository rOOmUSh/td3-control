use std::convert::TryFrom;

use crate::step::*;

#[test]
fn step_default_and_capacity() {
    let step = Step::default();
    assert_eq!(Step::COUNT, 16);
    assert_eq!(step.note, 0);
    assert_eq!(step.transpose, Transpose::Normal);
    assert_eq!(step.accent, Accent::Off);
    assert_eq!(step.slide, Slide::Off);
    assert_eq!(step.time, Time::Normal);
}

#[test]
fn transpose_try_from_u8_valid() {
    assert_eq!(Transpose::try_from(0u8), Ok(Transpose::Down));
    assert_eq!(Transpose::try_from(1u8), Ok(Transpose::Normal));
    assert_eq!(Transpose::try_from(2u8), Ok(Transpose::Up));
}

#[test]
fn transpose_try_from_u8_invalid() {
    assert!(Transpose::try_from(3u8).is_err());
    assert!(Transpose::try_from(255u8).is_err());
}

#[test]
fn transpose_repr_values() {
    assert_eq!(Transpose::Down as u8, 0);
    assert_eq!(Transpose::Normal as u8, 1);
    assert_eq!(Transpose::Up as u8, 2);
}

#[test]
fn transpose_contract_mapping() {
    assert_eq!(Transpose::Down.contract_name(), "DOWN");
    assert_eq!(Transpose::Normal.contract_name(), "NORMAL");
    assert_eq!(Transpose::Up.contract_name(), "UP");
    assert_eq!(Transpose::from_contract("DOWN"), Ok(Transpose::Down));
    assert_eq!(Transpose::from_contract("normal"), Ok(Transpose::Normal));
    assert_eq!(Transpose::from_contract("UP"), Ok(Transpose::Up));
}

#[test]
fn transpose_contract_rejects_non_contract_tokens() {
    assert!(Transpose::from_contract("DN").is_err());
    assert!(Transpose::from_contract("").is_err());
    assert!(Transpose::from_contract("XX").is_err());
}

#[test]
fn transpose_pitch_base_offsets() {
    assert_eq!(Transpose::Down.pitch_base_offset(), 0);
    assert_eq!(Transpose::Normal.pitch_base_offset(), 12);
    assert_eq!(Transpose::Up.pitch_base_offset(), 24);
}

#[test]
fn transpose_steps_symbol_mapping() {
    assert_eq!(Transpose::Down.steps_symbol(), b'D');
    assert_eq!(Transpose::Normal.steps_symbol(), b'-');
    assert_eq!(Transpose::Up.steps_symbol(), b'U');
    assert_eq!(Transpose::from_steps_symbol(b'D'), Ok(Transpose::Down));
    assert_eq!(Transpose::from_steps_symbol(b'-'), Ok(Transpose::Normal));
    assert_eq!(Transpose::from_steps_symbol(b'U'), Ok(Transpose::Up));
}

#[test]
fn transpose_steps_symbol_invalid() {
    assert!(Transpose::from_steps_symbol(b'X').is_err());
}

#[test]
fn accent_try_from_u8_valid() {
    assert_eq!(Accent::try_from(0u8), Ok(Accent::Off));
    assert_eq!(Accent::try_from(1u8), Ok(Accent::On));
}

#[test]
fn accent_try_from_u8_invalid() {
    assert!(Accent::try_from(2u8).is_err());
}

#[test]
fn accent_enabled_mapping() {
    assert!(!Accent::Off.enabled());
    assert!(Accent::On.enabled());
    assert_eq!(Accent::from_enabled(false), Accent::Off);
    assert_eq!(Accent::from_enabled(true), Accent::On);
}

#[test]
fn accent_steps_symbol_mapping() {
    assert_eq!(Accent::Off.steps_symbol(), b'-');
    assert_eq!(Accent::On.steps_symbol(), b'A');
    assert_eq!(Accent::from_steps_symbol(b'-'), Ok(Accent::Off));
    assert_eq!(Accent::from_steps_symbol(b'A'), Ok(Accent::On));
}

#[test]
fn accent_steps_symbol_invalid() {
    assert!(Accent::from_steps_symbol(b'X').is_err());
}

#[test]
fn slide_try_from_u8_valid() {
    assert_eq!(Slide::try_from(0u8), Ok(Slide::Off));
    assert_eq!(Slide::try_from(1u8), Ok(Slide::On));
}

#[test]
fn slide_try_from_u8_invalid() {
    assert!(Slide::try_from(2u8).is_err());
}

#[test]
fn slide_enabled_mapping() {
    assert!(!Slide::Off.enabled());
    assert!(Slide::On.enabled());
    assert_eq!(Slide::from_enabled(false), Slide::Off);
    assert_eq!(Slide::from_enabled(true), Slide::On);
}

#[test]
fn slide_steps_symbol_mapping() {
    assert_eq!(Slide::Off.steps_symbol(), b'-');
    assert_eq!(Slide::On.steps_symbol(), b'S');
    assert_eq!(Slide::from_steps_symbol(b'-'), Ok(Slide::Off));
    assert_eq!(Slide::from_steps_symbol(b'S'), Ok(Slide::On));
}

#[test]
fn slide_steps_symbol_invalid() {
    assert!(Slide::from_steps_symbol(b'X').is_err());
}

#[test]
fn time_try_from_u8_valid() {
    assert_eq!(Time::try_from(0b00u8), Ok(Time::Tie));
    assert_eq!(Time::try_from(0b01u8), Ok(Time::Normal));
    assert_eq!(Time::try_from(0b10u8), Ok(Time::TieRest));
    assert_eq!(Time::try_from(0b11u8), Ok(Time::Rest));
}

#[test]
fn time_try_from_u8_invalid() {
    assert!(Time::try_from(4u8).is_err());
}

#[test]
fn time_try_from_u16_valid() {
    assert_eq!(Time::try_from(0b00u16), Ok(Time::Tie));
    assert_eq!(Time::try_from(0b01u16), Ok(Time::Normal));
    assert_eq!(Time::try_from(0b10u16), Ok(Time::TieRest));
    assert_eq!(Time::try_from(0b11u16), Ok(Time::Rest));
}

#[test]
fn time_repr_values() {
    assert_eq!(Time::Tie as u8, 0b00);
    assert_eq!(Time::Normal as u8, 0b01);
    assert_eq!(Time::TieRest as u8, 0b10);
    assert_eq!(Time::Rest as u8, 0b11);
}

#[test]
fn time_contract_mapping() {
    assert_eq!(Time::Tie.contract_name(), "TIE");
    assert_eq!(Time::Normal.contract_name(), "NORMAL");
    assert_eq!(Time::TieRest.contract_name(), "TIE_REST");
    assert_eq!(Time::Rest.contract_name(), "REST");
    assert_eq!(Time::from_contract("TIE"), Ok(Time::Tie));
    assert_eq!(Time::from_contract("normal"), Ok(Time::Normal));
    assert_eq!(Time::from_contract("TIE_REST"), Ok(Time::TieRest));
    assert_eq!(Time::from_contract("REST"), Ok(Time::Rest));
}

#[test]
fn time_contract_rejects_non_contract_tokens() {
    assert!(Time::from_contract("TI").is_err());
    assert!(Time::from_contract("RE").is_err());
    assert!(Time::from_contract("TR").is_err());
    assert!(Time::from_contract("").is_err());
    assert!(Time::from_contract("XX").is_err());
}

#[test]
fn time_steps_token_mapping() {
    assert_eq!(Time::Normal.steps_token(), "N");
    assert_eq!(Time::Tie.steps_token(), "T");
    assert_eq!(Time::Rest.steps_token(), "R");
    assert_eq!(Time::TieRest.steps_token(), "TR");
    assert_eq!(Time::from_steps_token("N"), Ok(Time::Normal));
    assert_eq!(Time::from_steps_token("t"), Ok(Time::Tie));
    assert_eq!(Time::from_steps_token("R"), Ok(Time::Rest));
    assert_eq!(Time::from_steps_token("tr"), Ok(Time::TieRest));
}

#[test]
fn time_steps_token_invalid() {
    assert!(Time::from_steps_token("TI").is_err());
    assert!(Time::from_steps_token("").is_err());
    assert!(Time::from_steps_token("XX").is_err());
}
