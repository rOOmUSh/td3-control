//! StepDSL v1.1: per-step `CO`/`GT` lanes, lane switches, morph and LIVE
//! header keys, and the v1 compatibility rules.

use crate::formats::steps_txt::{self, StepsTxtExportMeta, StepsTxtMeta};
use crate::pattern::Pattern;

const LANES_FIXTURE: &str = include_str!("../../tests/fixtures/stepsdsl_v1_1_lanes.steps.txt");
const V1_FIXTURE: &str = include_str!("../../tests/fixtures/stepsdslv1_1.steps.txt");

fn v1_1_header(rows: &str) -> String {
    format!(
        "format=td3-stepdsl-v1.1\nactive_steps=4\ntriplet_time=off\nbpm=120\n\n{}\n",
        rows
    )
}

// ---------------------------------------------------------------------------
// Compatibility
// ---------------------------------------------------------------------------

#[test]
fn v1_document_parses_with_empty_meta() {
    let doc = steps_txt::import_document(V1_FIXTURE).unwrap();
    assert_eq!(doc.pattern.active_steps, 3);
    assert_eq!(doc.centibpm, Some(12_800));
    assert_eq!(
        doc.meta,
        StepsTxtMeta {
            centibpm: Some(12_800),
            ..Default::default()
        }
    );
}

#[test]
fn v1_document_still_rejects_unknown_header_keys() {
    let text = "format=td3-stepdsl-v1\nactive_steps=1\nmystery=1\n\n01  C:---:N\n";
    let err = steps_txt::import_document(text).unwrap_err().to_string();
    assert!(err.contains("line 3"), "got: {}", err);
}

#[test]
fn v1_1_document_ignores_unknown_header_keys() {
    let text = "format=td3-stepdsl-v1.1\nactive_steps=1\nmystery=1\n\n01  C:---:N\n";
    let doc = steps_txt::import_document(text).unwrap();
    assert_eq!(doc.pattern.active_steps, 1);
}

#[test]
fn unknown_format_tag_is_still_rejected() {
    let text = "format=td3-stepdsl-v2\nactive_steps=1\n\n01  C:---:N\n";
    assert!(steps_txt::import_document(text).is_err());
}

#[test]
fn export_without_meta_writes_defaults_on_every_row() {
    let text = steps_txt::export_with_bpm(&Pattern::default(), 12_000).unwrap();
    assert!(text.starts_with("format=td3-stepdsl-v1.1\n"));
    assert!(text.contains("triplet_morph=off\n"));
    assert!(text.contains("triplet_morph_percentage=0\n"));
    assert!(text.contains("live_update=off\n"));
    assert!(text.contains("pattern_co_lane=off\n"));
    assert!(text.contains("pattern_gt_lane=off\n"));
    assert!(text.contains("\n01  C:---:N|CO:64|GT:50\n"), "{}", text);
    assert!(text.contains("# Cutoff Control | CO:0-127\n"));
}

// ---------------------------------------------------------------------------
// Round trip
// ---------------------------------------------------------------------------

#[test]
fn golden_lanes_fixture_reads_every_field() {
    let doc = steps_txt::import_document(LANES_FIXTURE).unwrap();
    assert_eq!(doc.pattern.active_steps, 8);
    assert_eq!(doc.centibpm, Some(12_450));
    let cutoff = doc.meta.cutoff.expect("cutoff lane present");
    assert_eq!(&cutoff[..8], &[0, 40, 64, 90, 127, 100, 12, 77]);
    assert_eq!(
        &cutoff[8..],
        &[64; 8],
        "rows beyond active steps keep the default"
    );
    let gate = doc.meta.gate.expect("gate lane present");
    assert!(gate.iter().all(|g| *g == 50));
    assert_eq!(doc.meta.cutoff_lane_on, Some(true));
    assert_eq!(doc.meta.gate_lane_on, Some(false));
    assert_eq!(doc.meta.triplet_morph_percent, Some(40));
    assert_eq!(doc.meta.live_update, Some(false));
}

#[test]
fn export_with_meta_round_trips_lanes_switches_morph_and_live() {
    let mut cutoff = [64u8; 16];
    let mut gate = [50u8; 16];
    for i in 0..16 {
        cutoff[i] = (i * 8) as u8;
        gate[i] = (i * 6 + 1) as u8;
    }
    let meta = StepsTxtExportMeta {
        cutoff,
        gate,
        cutoff_lane_on: true,
        gate_lane_on: true,
        triplet_morph_percent: Some(69),
        live_update: true,
    };
    let pattern = Pattern::new(false, 16, Default::default()).unwrap();
    let text = steps_txt::export_with_meta(&pattern, 12_000, &meta).unwrap();
    assert!(
        text.contains("triplet_morph=on\ntriplet_morph_percentage=69\n"),
        "{}",
        text
    );
    assert!(text.contains("live_update=on\n"));
    assert!(text.contains("\n01  C:---:N|CO:0|GT:1\n"), "{}", text);

    let doc = steps_txt::import_document(&text).unwrap();
    assert_eq!(doc.meta.cutoff, Some(cutoff));
    assert_eq!(doc.meta.gate, Some(gate));
    assert_eq!(doc.meta.cutoff_lane_on, Some(true));
    assert_eq!(doc.meta.gate_lane_on, Some(true));
    assert_eq!(doc.meta.triplet_morph_percent, Some(69));
    assert_eq!(doc.meta.live_update, Some(true));
    assert_eq!(doc.centibpm, Some(12_000));
}

#[test]
fn golden_fixture_survives_export_and_reimport() {
    let doc = steps_txt::import_document(LANES_FIXTURE).unwrap();
    let meta = StepsTxtExportMeta {
        cutoff: doc.meta.cutoff.unwrap(),
        gate: doc.meta.gate.unwrap(),
        cutoff_lane_on: doc.meta.cutoff_lane_on.unwrap(),
        gate_lane_on: doc.meta.gate_lane_on.unwrap(),
        triplet_morph_percent: doc.meta.triplet_morph_percent,
        live_update: doc.meta.live_update.unwrap(),
    };
    let text = steps_txt::export_with_meta(&doc.pattern, doc.centibpm.unwrap(), &meta).unwrap();
    let again = steps_txt::import_document(&text).unwrap();
    assert_eq!(again.meta, doc.meta);
    assert_eq!(
        steps_txt::export(&again.pattern),
        steps_txt::export(&doc.pattern)
    );
}

// ---------------------------------------------------------------------------
// Lenient field rules
// ---------------------------------------------------------------------------

#[test]
fn out_of_range_values_clamp_to_the_nearest_valid_value() {
    let text = v1_1_header(
        "01  C:---:N|CO:200|GT:128\n02  C:---:N|CO:-5|GT:0\n03  C:---:N|CO:64|GT:50\n04  C:---:N|CO:64|GT:50",
    );
    let doc = steps_txt::import_document(&text).unwrap();
    let cutoff = doc.meta.cutoff.unwrap();
    let gate = doc.meta.gate.unwrap();
    assert_eq!(&cutoff[..2], &[127, 0]);
    assert_eq!(&gate[..2], &[100, 1]);
}

#[test]
fn a_non_numeric_or_missing_field_on_an_active_row_drops_that_lane_only() {
    let text = v1_1_header(
        "01  C:---:N|CO:10|GT:x\n02  C:---:N|CO:20|GT:50\n03  C:---:N|CO:30|GT:50\n04  C:---:N|CO:40|GT:50",
    );
    let doc = steps_txt::import_document(&text).unwrap();
    assert_eq!(doc.meta.cutoff.map(|c| c[3]), Some(40));
    assert_eq!(
        doc.meta.gate, None,
        "invalid GT on one row drops the gate lane"
    );
    assert_eq!(doc.meta.gate_lane_on, None);

    let text = v1_1_header(
        "01  C:---:N|CO:10|GT:50\n02  C:---:N|GT:50\n03  C:---:N|CO:30|GT:50\n04  C:---:N|CO:40|GT:50",
    );
    let doc = steps_txt::import_document(&text).unwrap();
    assert_eq!(
        doc.meta.cutoff, None,
        "missing CO on one row drops the cutoff lane"
    );
    assert!(doc.meta.gate.is_some());
}

#[test]
fn rows_beyond_active_steps_never_decide_lane_presence() {
    let text = "format=td3-stepdsl-v1.1\nactive_steps=2\n\n01  C:---:N|CO:10|GT:50\n02  C:---:N|CO:20|GT:50\n03  C:---:N\n".to_string();
    let doc = steps_txt::import_document(&text).unwrap();
    assert_eq!(doc.meta.cutoff.map(|c| c[1]), Some(20));
    assert_eq!(
        doc.meta.cutoff.map(|c| c[2]),
        Some(64),
        "absent row keeps the default"
    );
}

#[test]
fn unknown_row_fields_are_ignored() {
    let text = v1_1_header(
        "01  C:---:N|CO:10|XX:9|GT:50\n02  C:---:N|CO:10|GT:50\n03  C:---:N|CO:10|GT:50\n04  C:---:N|CO:10|GT:50",
    );
    let doc = steps_txt::import_document(&text).unwrap();
    assert_eq!(doc.meta.cutoff.map(|c| c[0]), Some(10));
}

#[test]
fn missing_lane_switch_keys_use_the_all_equal_heuristic() {
    let same = v1_1_header(
        "01  C:---:N|CO:10|GT:30\n02  C:---:N|CO:10|GT:50\n03  C:---:N|CO:10|GT:50\n04  C:---:N|CO:10|GT:50",
    );
    let doc = steps_txt::import_document(&same).unwrap();
    assert_eq!(
        doc.meta.cutoff_lane_on,
        Some(false),
        "all CO equal means off"
    );
    assert_eq!(doc.meta.gate_lane_on, Some(true), "one GT differs means on");
}

#[test]
fn explicit_lane_switch_keys_win_over_the_heuristic() {
    let text = "format=td3-stepdsl-v1.1\nactive_steps=2\npattern_co_lane=on\npattern_gt_lane=off\n\n01  C:---:N|CO:10|GT:30\n02  C:---:N|CO:10|GT:60\n";
    let doc = steps_txt::import_document(text).unwrap();
    assert_eq!(doc.meta.cutoff_lane_on, Some(true));
    assert_eq!(doc.meta.gate_lane_on, Some(false));
}

#[test]
fn morph_keys_need_both_on_and_a_usable_percentage() {
    let on_no_percent =
        "format=td3-stepdsl-v1.1\nactive_steps=1\ntriplet_morph=on\n\n01  C:---:N\n";
    assert_eq!(
        steps_txt::import_document(on_no_percent)
            .unwrap()
            .meta
            .triplet_morph_percent,
        None
    );
    let off_with_percent = "format=td3-stepdsl-v1.1\nactive_steps=1\ntriplet_morph=off\ntriplet_morph_percentage=50\n\n01  C:---:N\n";
    assert_eq!(
        steps_txt::import_document(off_with_percent)
            .unwrap()
            .meta
            .triplet_morph_percent,
        None
    );
    let clamped = "format=td3-stepdsl-v1.1\nactive_steps=1\ntriplet_morph=on\ntriplet_morph_percentage=250\n\n01  C:---:N\n";
    assert_eq!(
        steps_txt::import_document(clamped)
            .unwrap()
            .meta
            .triplet_morph_percent,
        Some(100)
    );
    let junk = "format=td3-stepdsl-v1.1\nactive_steps=1\ntriplet_morph=maybe\nlive_update=yes\n\n01  C:---:N\n";
    let doc = steps_txt::import_document(junk).unwrap();
    assert_eq!(doc.meta.triplet_morph_percent, None);
    assert_eq!(
        doc.meta.live_update, None,
        "unusable on/off value is absent"
    );
}
