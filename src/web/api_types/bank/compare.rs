use serde::{Deserialize, Serialize};

use crate::library::model::PatternRelation;
use crate::library::{
    DuplicateCluster, ItemCompareReport, MergePlan, RelatedGroup, SnapshotCompareReport,
};

#[derive(Deserialize)]
pub struct ItemCompareQuery {
    pub a: String,
    pub b: String,
}

#[derive(Serialize, Deserialize)]
pub struct ItemCompareResponse {
    pub report: ItemCompareReport,
}

#[derive(Deserialize)]
pub struct SnapshotCompareQuery {
    pub src: String,
    pub dst: String,
}

#[derive(Serialize, Deserialize)]
pub struct SnapshotCompareResponse {
    pub report: SnapshotCompareReport,
}

#[derive(Deserialize)]
pub struct MergePlanRequest {
    pub source_snapshot_id: String,
    pub target_snapshot_id: String,
    #[serde(default)]
    pub selection: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct MergePlanResponse {
    pub plan: MergePlan,
    /// `true` when produced by the preview endpoint. Default `false` keeps
    /// the existing `/merge-plan` shape unchanged for older clients.
    #[serde(default)]
    pub preview: bool,
}

/// Optional query parameters for `GET /api/bank/related`.
/// Currently only `kind` is supported; additional filters can be added here
/// without breaking existing callers because every field is `#[serde(default)]`.
#[derive(Deserialize, Default)]
pub struct RelatedQuery {
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct RelatedGroupsResponse {
    pub groups: Vec<RelatedGroup>,
    pub relations: Vec<PatternRelation>,
}

#[derive(Serialize, Deserialize)]
pub struct DuplicatesResponse {
    pub clusters: Vec<DuplicateCluster>,
}
