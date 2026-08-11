use super::*;

// ---------------------------------------------------------------------------
// POST /api/pattern/triplet-morph/plan
// ---------------------------------------------------------------------------
//
// MIDI-independent planning endpoint. Returns the deterministic triplet
// morph plan for one canonical WebPattern: per-beat pair selection,
// rational source and target offsets for all 16 boundaries, and the
// 12-cell endpoint projection with provenance and semantic roles. The
// browser caches this plan and performs only visual interpolation; Rust
// stays the only normative planner. An ineligible source returns
// `eligible: false` with a typed reason instead of an HTTP error, so
// the UI can explain why the knob is unavailable.

pub async fn pattern_triplet_morph_plan(
    payload: Result<Json<TripletMorphPlanRequest>, JsonRejection>,
) -> Result<Json<TripletMorphPlanResponse>, AppError> {
    let req = json_payload(payload, "triplet morph plan")?;
    let pattern = web_to_pattern(&req.pattern)?;

    let body = crate::triplet_morph::normalize_source(&pattern).and_then(|phrase| {
        let plan = crate::triplet_morph::plan_triplet_morph(&pattern)?;
        let endpoint = crate::triplet_morph::project_endpoint(&pattern, &phrase, &plan)?;
        Ok(TripletMorphPlanBody::from_plan(&plan, &endpoint))
    });

    match body {
        Ok(body) => Ok(Json(TripletMorphPlanResponse {
            eligible: true,
            reason: None,
            plan: Some(body),
        })),
        Err(reason) => Ok(Json(TripletMorphPlanResponse {
            eligible: false,
            reason: Some(reason.to_string()),
            plan: None,
        })),
    }
}
