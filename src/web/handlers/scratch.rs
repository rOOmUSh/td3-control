use super::*;

pub async fn scratch_pattern(State(state): State<MidiState>) -> Json<ScratchPatternResponse> {
    let s = &state.scratch;
    let side = if s.side == 0 { "A" } else { "B" };
    Json(ScratchPatternResponse {
        patgroup: s.patgroup + 1,
        pattern: s.slot + 1,
        side: side.to_string(),
        label: format!("G{}-P{}{}", s.patgroup + 1, s.slot + 1, side),
    })
}
