//! HTTP-level smoke tests for `/api/bank/*` handlers.
//!
//! Uses `tower::ServiceExt::oneshot` exactly like `web_tests.rs` does, against
//! a miniature router that nests only the bank routes. Every test uses a
//! unique temp-file-backed library so tests can run in parallel.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use tower::ServiceExt;

use crate::library::LibraryStore;
use crate::web::api_types::{
    AddItemToSnapshotResponse, BankItemResponse, BankItemsResponse, BankPatternSaveEntry,
    CreateSnapshotFromPatternsRequest, CreateSnapshotRequest, DeleteBankItemResponse,
    DeleteSnapshotResponse, SavePatternsToBankRequest, SavePatternsToBankResponse,
    SnapshotDetailResponse, SnapshotFromPatternSlot, SnapshotsResponse, TagsResponse, WebNote,
    WebPattern, WebStep, WebTime, WebTranspose,
};
use crate::web::bank_handlers;
use crate::web::state::{AppState, ScratchSlot, UiConfigSnapshot};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_library() -> Arc<LibraryStore> {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let path = std::env::temp_dir().join(format!(
        "td3-bank-api-test-{}-{}-{}.json",
        pid,
        n,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_file(&path);
    Arc::new(LibraryStore::load_or_create(path).expect("test library"))
}

fn build_bank_router() -> Router {
    let state = AppState::for_tests(
        ScratchSlot {
            patgroup: 0,
            slot: 0,
            side: 0,
        },
        temp_library(),
        String::new(),
        UiConfigSnapshot::for_tests(),
        std::path::PathBuf::from("TD3_CONFIG.env"),
    );
    Router::new()
        .nest("/api", bank_handlers::router())
        .with_state(state)
}

fn seed_basic_item(library: &Arc<LibraryStore>, item_id: &str) {
    let item = crate::library::model::LibraryItem {
        item_id: item_id.into(),
        display_name: "Seeded".into(),
        source_kind: crate::library::model::SourceKind::File,
        source_label: "seed".into(),
        source_path: None,
        created_at: "20260101T000000Z".into(),
        updated_at: "20260101T000000Z".into(),
        tags: vec![],
        favorite: false,
        archived: false,
        slot_key: None,
        snapshot_id: None,
        snapshot_name: None,
        format: Some("seq".into()),
        scale_name: None,
        root_note: None,
        duplicate_status: crate::library::model::DuplicateStatus::Unknown,
        related_group_count: 0,
        analysis_status: crate::library::model::AnalysisStatus::Unknown,
        notes: None,
        content_hash: None,
    };
    library.upsert_item(item).unwrap();
}

#[tokio::test]
async fn get_items_empty_returns_empty_list() {
    let app = build_bank_router();
    let req = Request::builder()
        .uri("/api/bank/items")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let payload: BankItemsResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload.total, 0);
    assert!(payload.items.is_empty());
}

#[tokio::test]
async fn get_items_unknown_returns_400() {
    let app = build_bank_router();
    let req = Request::builder()
        .uri("/api/bank/items/does-not-exist")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn delete_item_route_removes_item_and_rejects_second_delete() {
    let library = temp_library();
    let state = AppState::for_tests(
        ScratchSlot {
            patgroup: 0,
            slot: 0,
            side: 0,
        },
        library.clone(),
        String::new(),
        UiConfigSnapshot::for_tests(),
        std::path::PathBuf::from("TD3_CONFIG.env"),
    );
    let app = Router::new()
        .nest("/api", bank_handlers::router())
        .with_state(state);
    seed_basic_item(&library, "delete_me");

    let req = Request::builder()
        .method("DELETE")
        .uri("/api/bank/items/delete_me/delete")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let deleted: DeleteBankItemResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(deleted.item_id, "delete_me");
    assert!(deleted.deleted);

    let req = Request::builder()
        .uri("/api/bank/items/delete_me")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let req = Request::builder()
        .method("DELETE")
        .uri("/api/bank/items/delete_me/delete")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn add_item_to_snapshot_creates_timestamp_snapshot_when_none_exist() {
    let library = temp_library();
    let state = AppState::for_tests(
        ScratchSlot {
            patgroup: 0,
            slot: 0,
            side: 0,
        },
        library.clone(),
        String::new(),
        UiConfigSnapshot::for_tests(),
        std::path::PathBuf::from("TD3_CONFIG.env"),
    );
    let app = Router::new()
        .nest("/api", bank_handlers::router())
        .with_state(state);
    seed_basic_item(&library, "snap_item");

    let req = Request::builder()
        .method("POST")
        .uri("/api/bank/items/snap_item/add-to-snapshot")
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let payload: AddItemToSnapshotResponse = serde_json::from_slice(&body).unwrap();
    assert!(payload.created_snapshot);
    assert_eq!(payload.item_id, "snap_item");
    assert!(payload.snapshot.name.starts_with("SN_"));
    assert_eq!(payload.snapshot.slot_count, 1);
    assert_eq!(payload.slot.slot_key, "G1-P1A");
    assert_eq!(payload.slot.item_id.as_deref(), Some("snap_item"));
    assert_eq!(payload.slots.len(), 64);

    let req = Request::builder()
        .uri(format!(
            "/api/bank/snapshots/{}",
            payload.snapshot.snapshot_id
        ))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let detail: SnapshotDetailResponse = serde_json::from_slice(&body).unwrap();
    let slot = detail
        .slots
        .iter()
        .find(|slot| slot.slot_key == "G1-P1A")
        .unwrap();
    assert_eq!(slot.item_id.as_deref(), Some("snap_item"));
    assert_eq!(detail.snapshot.slot_count, 1);
}

#[tokio::test]
async fn add_item_to_snapshot_rejects_occupied_slot() {
    let library = temp_library();
    let state = AppState::for_tests(
        ScratchSlot {
            patgroup: 0,
            slot: 0,
            side: 0,
        },
        library.clone(),
        String::new(),
        UiConfigSnapshot::for_tests(),
        std::path::PathBuf::from("TD3_CONFIG.env"),
    );
    let app = Router::new()
        .nest("/api", bank_handlers::router())
        .with_state(state);
    seed_basic_item(&library, "first_item");
    seed_basic_item(&library, "second_item");
    let snap = library
        .create_snapshot(
            "Existing".into(),
            None,
            crate::library::model::SnapshotOrigin::Manual,
        )
        .unwrap();

    let body = serde_json::to_vec(&serde_json::json!({
        "snapshot_id": snap.snapshot_id,
        "slot_key": "G1-P1A"
    }))
    .unwrap();
    let req = Request::builder()
        .method("POST")
        .uri("/api/bank/items/first_item/add-to-snapshot")
        .header("content-type", "application/json")
        .body(Body::from(body.clone()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let req = Request::builder()
        .method("POST")
        .uri("/api/bank/items/second_item/add-to-snapshot")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_snapshot_then_get_details() {
    let app = build_bank_router();

    let create = CreateSnapshotRequest {
        name: "SnapshotCreateTest".to_string(),
        description: Some("hello".into()),
        origin: crate::library::model::SnapshotOrigin::Manual,
    };
    let body = serde_json::to_vec(&create).unwrap();
    let req = Request::builder()
        .method("POST")
        .uri("/api/bank/snapshots")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let created: SnapshotDetailResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(created.snapshot.name, "SnapshotCreateTest");
    assert_eq!(created.slots.len(), 64);

    // GET /snapshots should return our created snapshot
    let req = Request::builder()
        .uri("/api/bank/snapshots")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let list: SnapshotsResponse = serde_json::from_slice(&body).unwrap();
    assert!(list
        .snapshots
        .iter()
        .any(|s| s.name == "SnapshotCreateTest"));

    // GET /snapshots/:id
    let id = created.snapshot.snapshot_id.clone();
    let req = Request::builder()
        .uri(format!("/api/bank/snapshots/{}", id))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let detail: SnapshotDetailResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(detail.snapshot.snapshot_id, id);
    assert_eq!(detail.slots.len(), 64);
}

// -------------------------------------------------------------------------
// /bank/snapshots/from-patterns - main-page PUSH overflow flow.
// -------------------------------------------------------------------------

fn sample_web_pattern() -> WebPattern {
    WebPattern {
        active_steps: 16,
        triplet: false,
        steps: [WebStep {
            note: WebNote::C,
            transpose: WebTranspose::Normal,
            accent: false,
            slide: false,
            time: WebTime::Normal,
        }; 16],
    }
}

fn canonical_slot_keys_alt() -> Vec<String> {
    // ALTERNATE: G1P1A, G1P1B, G1P2A, ..., G4P8A, G4P8B - dashed form.
    let mut out = Vec::with_capacity(64);
    for g in 1..=4 {
        for p in 1..=8 {
            out.push(format!("G{}-P{}A", g, p));
            out.push(format!("G{}-P{}B", g, p));
        }
    }
    out
}

#[tokio::test]
async fn save_patterns_to_bank_single_item_persists_root_scale_tags() {
    let app = build_bank_router();
    let req = SavePatternsToBankRequest {
        destination: "single_item".into(),
        snapshot_id: None,
        snapshot_name: None,
        description: None,
        root_note: Some("D".into()),
        scale_name: Some("phrygian_dominant".into()),
        entries: vec![BankPatternSaveEntry {
            pattern: sample_web_pattern(),
            display_name: Some("P1 G1P1A".into()),
            slot_key: Some("G1-P1A".into()),
        }],
    };
    let body = serde_json::to_vec(&req).unwrap();
    let http = Request::builder()
        .method("POST")
        .uri("/api/bank/patterns/save")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(http).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let saved: SavePatternsToBankResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(saved.items.len(), 1);
    assert!(saved.snapshot.is_none());
    assert_eq!(saved.items[0].display_name, "P1 G1P1A");
    assert_eq!(saved.items[0].root_note.as_deref(), Some("D"));
    assert_eq!(
        saved.items[0].scale_name.as_deref(),
        Some("phrygian_dominant")
    );
    assert!(saved.items[0].tags.iter().any(|t| t == "root:D"));
    assert!(saved.items[0]
        .tags
        .iter()
        .any(|t| t == "scale:phrygian_dominant"));
}

#[tokio::test]
async fn save_patterns_to_bank_new_snapshot_uses_preferred_slots() {
    let app = build_bank_router();
    let req = SavePatternsToBankRequest {
        destination: "new_snapshot".into(),
        snapshot_id: None,
        snapshot_name: Some("SN_test_canvas".into()),
        description: None,
        root_note: Some("C".into()),
        scale_name: Some("minor".into()),
        entries: vec![
            BankPatternSaveEntry {
                pattern: sample_web_pattern(),
                display_name: Some("P1 G1P1B".into()),
                slot_key: Some("G1-P1B".into()),
            },
            BankPatternSaveEntry {
                pattern: sample_web_pattern(),
                display_name: Some("P2 G1P2A".into()),
                slot_key: Some("G1-P2A".into()),
            },
        ],
    };
    let body = serde_json::to_vec(&req).unwrap();
    let http = Request::builder()
        .method("POST")
        .uri("/api/bank/patterns/save")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(http).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let saved: SavePatternsToBankResponse = serde_json::from_slice(&body).unwrap();
    assert!(saved.created_snapshot);
    assert_eq!(
        saved.snapshot.as_ref().map(|s| s.name.as_str()),
        Some("SN_test_canvas")
    );
    let filled: Vec<_> = saved
        .slots
        .iter()
        .filter(|slot| !slot.empty)
        .map(|slot| slot.slot_key.as_str())
        .collect();
    assert_eq!(filled, vec!["G1-P1B", "G1-P2A"]);
}

#[tokio::test]
async fn save_patterns_to_bank_rejects_malformed_slot_key() {
    let app = build_bank_router();
    let req = SavePatternsToBankRequest {
        destination: "new_snapshot".into(),
        snapshot_id: None,
        snapshot_name: Some("bad-slot".into()),
        description: None,
        root_note: None,
        scale_name: None,
        entries: vec![BankPatternSaveEntry {
            pattern: sample_web_pattern(),
            display_name: None,
            slot_key: Some("G1P1A".into()),
        }],
    };
    let body = serde_json::to_vec(&req).unwrap();
    let http = Request::builder()
        .method("POST")
        .uri("/api/bank/patterns/save")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(http).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_snapshot_from_patterns_success_persists_64_slots() {
    let app = build_bank_router();
    let keys = canonical_slot_keys_alt();
    let slots: Vec<SnapshotFromPatternSlot> = keys
        .into_iter()
        .map(|slot_key| SnapshotFromPatternSlot {
            slot_key,
            pattern: sample_web_pattern(),
            display_name: None,
        })
        .collect();
    let req = CreateSnapshotFromPatternsRequest {
        name: "overflow-success".into(),
        description: Some("overflow snapshot test".into()),
        slots,
    };
    let body = serde_json::to_vec(&req).unwrap();
    let http = Request::builder()
        .method("POST")
        .uri("/api/bank/snapshots/from-patterns")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(http).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "expected 200 OK");
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let detail: SnapshotDetailResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(detail.snapshot.name, "overflow-success");
    // Catalog always materialises the 64-slot grid; every slot_key we sent
    // should be present and filled (empty=false, item_id=Some).
    assert_eq!(detail.slots.len(), 64);
    let filled = detail
        .slots
        .iter()
        .filter(|s| !s.empty && s.item_id.is_some())
        .count();
    assert_eq!(filled, 64, "expected all 64 slots filled, got {}", filled);
}

#[tokio::test]
async fn create_snapshot_from_patterns_rejects_malformed_slot_key() {
    let app = build_bank_router();
    // Note: undashed `G1P1A` is the frontend form - the endpoint must
    // reject it because it's not the store's canonical shape.
    let slots = vec![SnapshotFromPatternSlot {
        slot_key: "G1P1A".into(),
        pattern: sample_web_pattern(),
        display_name: None,
    }];
    let req = CreateSnapshotFromPatternsRequest {
        name: "should-fail".into(),
        description: None,
        slots,
    };
    let body = serde_json::to_vec(&req).unwrap();
    let http = Request::builder()
        .method("POST")
        .uri("/api/bank/snapshots/from-patterns")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(http).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_snapshot_from_patterns_rejects_duplicate_slot_keys() {
    let app = build_bank_router();
    let slots = vec![
        SnapshotFromPatternSlot {
            slot_key: "G1-P1A".into(),
            pattern: sample_web_pattern(),
            display_name: None,
        },
        SnapshotFromPatternSlot {
            slot_key: "G1-P1A".into(),
            pattern: sample_web_pattern(),
            display_name: None,
        },
    ];
    let req = CreateSnapshotFromPatternsRequest {
        name: "dupes".into(),
        description: None,
        slots,
    };
    let body = serde_json::to_vec(&req).unwrap();
    let http = Request::builder()
        .method("POST")
        .uri("/api/bank/snapshots/from-patterns")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(http).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_snapshot_from_patterns_rejects_empty_name_and_empty_slots() {
    let app = build_bank_router();

    // Empty name.
    let req = CreateSnapshotFromPatternsRequest {
        name: "   ".into(),
        description: None,
        slots: vec![SnapshotFromPatternSlot {
            slot_key: "G1-P1A".into(),
            pattern: sample_web_pattern(),
            display_name: None,
        }],
    };
    let body = serde_json::to_vec(&req).unwrap();
    let http = Request::builder()
        .method("POST")
        .uri("/api/bank/snapshots/from-patterns")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.clone().oneshot(http).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // Empty slots.
    let req = CreateSnapshotFromPatternsRequest {
        name: "ok".into(),
        description: None,
        slots: vec![],
    };
    let body = serde_json::to_vec(&req).unwrap();
    let http = Request::builder()
        .method("POST")
        .uri("/api/bank/snapshots/from-patterns")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(http).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_snapshot_from_patterns_rejects_invalid_pattern_decode() {
    let app = build_bank_router();
    // Pattern with active_steps=0 is rejected by web_to_pattern. If any slot
    // is malformed the handler must reject the whole request before any
    // catalog row is written - atomicity invariant.
    let mut bad = sample_web_pattern();
    bad.active_steps = 0;
    let slots = vec![
        SnapshotFromPatternSlot {
            slot_key: "G1-P1A".into(),
            pattern: sample_web_pattern(),
            display_name: None,
        },
        SnapshotFromPatternSlot {
            slot_key: "G1-P1B".into(),
            pattern: bad,
            display_name: None,
        },
    ];
    let req = CreateSnapshotFromPatternsRequest {
        name: "should-not-land".into(),
        description: None,
        slots,
    };
    let body = serde_json::to_vec(&req).unwrap();
    let http = Request::builder()
        .method("POST")
        .uri("/api/bank/snapshots/from-patterns")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.clone().oneshot(http).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // Critical invariant: the rejected request must not have created a
    // half-populated snapshot. List snapshots and confirm the name is absent.
    let list_req = Request::builder()
        .uri("/api/bank/snapshots")
        .body(Body::empty())
        .unwrap();
    let list_resp = app.oneshot(list_req).await.unwrap();
    let list_body = list_resp.into_body().collect().await.unwrap().to_bytes();
    let list: SnapshotsResponse = serde_json::from_slice(&list_body).unwrap();
    assert!(
        !list.snapshots.iter().any(|s| s.name == "should-not-land"),
        "atomicity violated: rejected request left a snapshot behind",
    );
}

#[tokio::test]
async fn create_snapshot_from_patterns_name_collision_appends_suffix() {
    let app = build_bank_router();
    let slots = vec![SnapshotFromPatternSlot {
        slot_key: "G1-P1A".into(),
        pattern: sample_web_pattern(),
        display_name: None,
    }];

    // First create - name lands verbatim.
    let req = CreateSnapshotFromPatternsRequest {
        name: "main-overflow-1970-01-01".into(),
        description: None,
        slots: slots.clone(),
    };
    let body = serde_json::to_vec(&req).unwrap();
    let http = Request::builder()
        .method("POST")
        .uri("/api/bank/snapshots/from-patterns")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.clone().oneshot(http).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let first: SnapshotDetailResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(first.snapshot.name, "main-overflow-1970-01-01");

    // Second create with identical name - should land as "... (2)".
    let req = CreateSnapshotFromPatternsRequest {
        name: "main-overflow-1970-01-01".into(),
        description: None,
        slots,
    };
    let body = serde_json::to_vec(&req).unwrap();
    let http = Request::builder()
        .method("POST")
        .uri("/api/bank/snapshots/from-patterns")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(http).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let second: SnapshotDetailResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(second.snapshot.name, "main-overflow-1970-01-01 (2)");
    // IDs must differ - confirms a brand-new snapshot, not a rename.
    assert_ne!(first.snapshot.snapshot_id, second.snapshot.snapshot_id);
}

#[tokio::test]
async fn create_snapshot_from_patterns_dedupes_identical_patterns() {
    let app = build_bank_router();
    // All 4 slots share the same pattern content → dedupe must collapse
    // them to a single LibraryItem (the 4 snapshot_slots rows all point at
    // the same item_id).
    let slots = vec![
        SnapshotFromPatternSlot {
            slot_key: "G1-P1A".into(),
            pattern: sample_web_pattern(),
            display_name: None,
        },
        SnapshotFromPatternSlot {
            slot_key: "G1-P1B".into(),
            pattern: sample_web_pattern(),
            display_name: None,
        },
        SnapshotFromPatternSlot {
            slot_key: "G1-P2A".into(),
            pattern: sample_web_pattern(),
            display_name: None,
        },
        SnapshotFromPatternSlot {
            slot_key: "G1-P2B".into(),
            pattern: sample_web_pattern(),
            display_name: None,
        },
    ];
    let req = CreateSnapshotFromPatternsRequest {
        name: "dedupe".into(),
        description: None,
        slots,
    };
    let body = serde_json::to_vec(&req).unwrap();
    let http = Request::builder()
        .method("POST")
        .uri("/api/bank/snapshots/from-patterns")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.clone().oneshot(http).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let detail: SnapshotDetailResponse = serde_json::from_slice(&body).unwrap();
    // All 4 filled slots should point at the same item_id.
    let filled: Vec<&Option<String>> = detail
        .slots
        .iter()
        .filter(|s| !s.empty)
        .map(|s| &s.item_id)
        .collect();
    assert_eq!(filled.len(), 4, "expected 4 filled slots");
    let first_id = filled[0].as_ref().expect("item_id");
    for id in &filled[1..] {
        assert_eq!(id.as_ref().expect("item_id"), first_id, "dedupe failed");
    }
}

#[tokio::test]
async fn compare_snapshots_unknown_snapshot_returns_400() {
    let app = build_bank_router();
    let req = Request::builder()
        .uri("/api/bank/compare/snapshots?src=missing-a&dst=missing-b")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn merge_plan_unknown_snapshot_returns_400() {
    let app = build_bank_router();
    let body = serde_json::to_vec(&serde_json::json!({
        "source_snapshot_id": "missing-a",
        "target_snapshot_id": "missing-b",
        "selection": [],
    }))
    .unwrap();
    let req = Request::builder()
        .method("POST")
        .uri("/api/bank/merge-plan")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn snapshot_detail_returns_64_slots() {
    let app = build_bank_router();

    let create = CreateSnapshotRequest {
        name: "GridTest".to_string(),
        description: None,
        origin: crate::library::model::SnapshotOrigin::Manual,
    };
    let body = serde_json::to_vec(&create).unwrap();
    let req = Request::builder()
        .method("POST")
        .uri("/api/bank/snapshots")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let created: SnapshotDetailResponse = serde_json::from_slice(&body).unwrap();
    let id = created.snapshot.snapshot_id.clone();

    let req = Request::builder()
        .uri(format!("/api/bank/snapshots/{}", id))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let detail: SnapshotDetailResponse = serde_json::from_slice(&body).unwrap();

    // Exactly 64 slot views, all empty, in canonical order G1-P1A .. G4-P8B.
    assert_eq!(detail.slots.len(), 64);
    let mut expected: Vec<String> = Vec::with_capacity(64);
    for g in 1..=4u8 {
        for p in 1..=8u8 {
            for side in ['A', 'B'] {
                expected.push(format!("G{}-P{}{}", g, p, side));
            }
        }
    }
    let actual: Vec<String> = detail.slots.iter().map(|s| s.slot_key.clone()).collect();
    assert_eq!(actual, expected);
    assert!(detail.slots.iter().all(|s| s.empty));
    // Compare markers are unset when no compare context is active.
    assert!(detail.slots.iter().all(|s| s.changed.is_none()));
    assert!(detail.slots.iter().all(|s| s.duplicate.is_none()));
}

#[tokio::test]
async fn sync_backups_missing_dir_returns_400() {
    let app = build_bank_router();

    let body = serde_json::to_vec(&serde_json::json!({
        "backup_dir": "/no-such-path-td3-ui"
    }))
    .unwrap();
    let req = Request::builder()
        .method("POST")
        .uri("/api/bank/snapshots/sync-backups")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn post_tag_then_list_items_filtered_by_tag() {
    // We need to seed an item first. Do that by constructing a direct
    // handler state and calling the library, then exercise the HTTP path.
    let library = temp_library();
    let state = AppState::for_tests(
        ScratchSlot {
            patgroup: 0,
            slot: 0,
            side: 0,
        },
        library.clone(),
        String::new(),
        UiConfigSnapshot::for_tests(),
        std::path::PathBuf::from("TD3_CONFIG.env"),
    );
    let app = Router::new()
        .nest("/api", bank_handlers::router())
        .with_state(state);

    // Seed one item.
    let item = crate::library::model::LibraryItem {
        item_id: "seed_item".into(),
        display_name: "Seeded".into(),
        source_kind: crate::library::model::SourceKind::File,
        source_label: "seed".into(),
        source_path: None,
        created_at: "20260101T000000Z".into(),
        updated_at: "20260101T000000Z".into(),
        tags: vec![],
        favorite: false,
        archived: false,
        slot_key: None,
        snapshot_id: None,
        snapshot_name: None,
        format: Some("seq".into()),
        scale_name: None,
        root_note: None,
        duplicate_status: crate::library::model::DuplicateStatus::Unknown,
        related_group_count: 0,
        analysis_status: crate::library::model::AnalysisStatus::Unknown,
        notes: None,
        content_hash: None,
    };
    library.upsert_item(item).unwrap();

    // POST /api/bank/items/seed_item/tags { label: "acid" }
    let body = serde_json::to_vec(&serde_json::json!({ "label": "acid" })).unwrap();
    let req = Request::builder()
        .method("POST")
        .uri("/api/bank/items/seed_item/tags")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // GET /api/bank/tags
    let req = Request::builder()
        .uri("/api/bank/tags")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let tags: TagsResponse = serde_json::from_slice(&body).unwrap();
    assert!(tags.tags.iter().any(|t| t.label == "acid"));

    // GET /api/bank/items?tag=acid
    let req = Request::builder()
        .uri("/api/bank/items?tag=acid")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let items: BankItemsResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(items.total, 1);
    assert_eq!(items.items[0].item_id, "seed_item");
    assert!(items.items[0].tags.contains(&"acid".to_string()));

    // GET /api/bank/items/seed_item
    let req = Request::builder()
        .uri("/api/bank/items/seed_item")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let payload: BankItemResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload.item.item_id, "seed_item");
}

// ---------------------------------------------------------------------------
// Snapshot slot deletion (DELETE /api/bank/snapshots/:id/slots)
// ---------------------------------------------------------------------------

async fn create_three_slot_snapshot(app: &Router) -> SnapshotDetailResponse {
    let slots = vec![
        SnapshotFromPatternSlot {
            slot_key: "G1-P1A".into(),
            pattern: sample_web_pattern(),
            display_name: Some("ACID".into()),
        },
        SnapshotFromPatternSlot {
            slot_key: "G1-P1B".into(),
            pattern: sample_web_pattern(),
            display_name: Some("BSL".into()),
        },
        SnapshotFromPatternSlot {
            slot_key: "G1-P2A".into(),
            pattern: sample_web_pattern(),
            display_name: Some("LEAD".into()),
        },
    ];
    let req = CreateSnapshotFromPatternsRequest {
        name: "delete-slots-fixture".into(),
        description: None,
        slots,
    };
    let body = serde_json::to_vec(&req).unwrap();
    let http = Request::builder()
        .method("POST")
        .uri("/api/bank/snapshots/from-patterns")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.clone().oneshot(http).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn delete_snapshot_slots_removes_listed_slots_and_keeps_64_grid() {
    let app = build_bank_router();
    let detail = create_three_slot_snapshot(&app).await;
    let snap_id = detail.snapshot.snapshot_id.clone();

    let body = serde_json::to_vec(&serde_json::json!({
        "slot_keys": ["G1-P1A", "G1-P1B"],
    }))
    .unwrap();
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/bank/snapshots/{}/slots", snap_id))
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Re-fetch detail and confirm the two listed slots are now empty,
    // but the third (G1-P2A) is still filled, and the grid is still 64.
    let req = Request::builder()
        .uri(format!("/api/bank/snapshots/{}", snap_id))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let after: SnapshotDetailResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(after.slots.len(), 64);
    let g1p1a = after.slots.iter().find(|s| s.slot_key == "G1-P1A").unwrap();
    let g1p1b = after.slots.iter().find(|s| s.slot_key == "G1-P1B").unwrap();
    let g1p2a = after.slots.iter().find(|s| s.slot_key == "G1-P2A").unwrap();
    assert!(g1p1a.empty && g1p1a.item_id.is_none());
    assert!(g1p1b.empty && g1p1b.item_id.is_none());
    assert!(!g1p2a.empty && g1p2a.item_id.is_some());
    assert_eq!(after.snapshot.slot_count, 1);
}

#[tokio::test]
async fn delete_snapshot_removes_snapshot_slots_and_owned_items() {
    let app = build_bank_router();
    let detail = create_three_slot_snapshot(&app).await;
    let snap_id = detail.snapshot.snapshot_id.clone();

    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/bank/snapshots/{}", snap_id))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let deleted: DeleteSnapshotResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(deleted.snapshot_id, snap_id);
    assert_eq!(deleted.removed_slots, 3);
    assert_eq!(deleted.removed_items, 1);

    let req = Request::builder()
        .uri("/api/bank/snapshots")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let list: SnapshotsResponse = serde_json::from_slice(&body).unwrap();
    assert!(!list.snapshots.iter().any(|s| s.snapshot_id == snap_id));

    let req = Request::builder()
        .uri(format!("/api/bank/snapshots/{}", snap_id))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let req = Request::builder()
        .uri("/api/bank/items")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let items: BankItemsResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(items.total, 0);
}

#[tokio::test]
async fn delete_snapshot_slots_rejects_malformed_keys() {
    let app = build_bank_router();
    let detail = create_three_slot_snapshot(&app).await;

    // Undashed form should be rejected - same canonical-shape rule as
    // create_snapshot_from_patterns.
    let body = serde_json::to_vec(&serde_json::json!({ "slot_keys": ["G1P1A"] })).unwrap();
    let req = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/api/bank/snapshots/{}/slots",
            detail.snapshot.snapshot_id
        ))
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn delete_snapshot_slots_rejects_empty_list() {
    let app = build_bank_router();
    let detail = create_three_slot_snapshot(&app).await;

    let body = serde_json::to_vec(&serde_json::json!({ "slot_keys": [] })).unwrap();
    let req = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/api/bank/snapshots/{}/slots",
            detail.snapshot.snapshot_id
        ))
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn delete_snapshot_slots_unknown_snapshot_returns_400() {
    let app = build_bank_router();
    let body = serde_json::to_vec(&serde_json::json!({ "slot_keys": ["G1-P1A"] })).unwrap();
    let req = Request::builder()
        .method("DELETE")
        .uri("/api/bank/snapshots/snap_does_not_exist/slots")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// Snapshot slot move/swap (POST /api/bank/snapshots/:id/move-slot)
// ---------------------------------------------------------------------------

async fn post_move_slot(
    app: &Router,
    snapshot_id: &str,
    from_key: &str,
    to_key: &str,
) -> axum::http::Response<Body> {
    let body = serde_json::to_vec(&serde_json::json!({
        "from_key": from_key,
        "to_key": to_key,
    }))
    .unwrap();
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/bank/snapshots/{}/move-slot", snapshot_id))
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    app.clone().oneshot(req).await.unwrap()
}

#[tokio::test]
async fn move_snapshot_slot_into_empty_destination_renames_in_place() {
    let app = build_bank_router();
    let detail = create_three_slot_snapshot(&app).await;
    let snap_id = detail.snapshot.snapshot_id.clone();
    let initial_count = detail.snapshot.slot_count;
    let initial_filled = detail.slots.iter().filter(|s| !s.empty).count();

    // G1-P1A ("ACID") → G2-P3B (empty). Total number of occupied rows is
    // conserved by a move, so the visible-grid count is unchanged.
    let resp = post_move_slot(&app, &snap_id, "G1-P1A", "G2-P3B").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let payload: crate::web::api_types::MoveSnapshotSlotResponse =
        serde_json::from_slice(&body).unwrap();
    assert!(!payload.swapped);
    assert_eq!(payload.snapshot.slot_count, initial_count);
    let after_filled = payload.slots.iter().filter(|s| !s.empty).count();
    assert_eq!(after_filled, initial_filled);

    let dest = payload
        .slots
        .iter()
        .find(|s| s.slot_key == "G2-P3B")
        .unwrap();
    let src = payload
        .slots
        .iter()
        .find(|s| s.slot_key == "G1-P1A")
        .unwrap();
    assert!(!dest.empty);
    assert_eq!(dest.display_name.as_deref(), Some("ACID"));
    assert!(src.empty);
}

#[tokio::test]
async fn move_snapshot_slot_into_occupied_destination_swaps() {
    let app = build_bank_router();
    let detail = create_three_slot_snapshot(&app).await;
    let snap_id = detail.snapshot.snapshot_id.clone();
    let initial_filled = detail.slots.iter().filter(|s| !s.empty).count();

    // G1-P1A ("ACID") <-> G1-P2A ("LEAD"). After the swap LEAD is at G1-P1A
    // and ACID at G1-P2A; everything else is unchanged.
    let resp = post_move_slot(&app, &snap_id, "G1-P1A", "G1-P2A").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let payload: crate::web::api_types::MoveSnapshotSlotResponse =
        serde_json::from_slice(&body).unwrap();
    assert!(payload.swapped);
    let after_filled = payload.slots.iter().filter(|s| !s.empty).count();
    assert_eq!(after_filled, initial_filled);

    let p1a = payload
        .slots
        .iter()
        .find(|s| s.slot_key == "G1-P1A")
        .unwrap();
    let p2a = payload
        .slots
        .iter()
        .find(|s| s.slot_key == "G1-P2A")
        .unwrap();
    let p1b = payload
        .slots
        .iter()
        .find(|s| s.slot_key == "G1-P1B")
        .unwrap();
    assert_eq!(p1a.display_name.as_deref(), Some("LEAD"));
    assert_eq!(p2a.display_name.as_deref(), Some("ACID"));
    // Untouched slot stays put.
    assert_eq!(p1b.display_name.as_deref(), Some("BSL"));
}

#[tokio::test]
async fn move_snapshot_slot_rejects_empty_source() {
    let app = build_bank_router();
    let detail = create_three_slot_snapshot(&app).await;
    let snap_id = detail.snapshot.snapshot_id.clone();

    // G4-P8B is empty in the fixture.
    let resp = post_move_slot(&app, &snap_id, "G4-P8B", "G1-P1A").await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn move_snapshot_slot_rejects_malformed_keys() {
    let app = build_bank_router();
    let detail = create_three_slot_snapshot(&app).await;
    let snap_id = detail.snapshot.snapshot_id.clone();

    // Undashed source.
    let resp = post_move_slot(&app, &snap_id, "G1P1A", "G2-P3B").await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    // Undashed destination.
    let resp = post_move_slot(&app, &snap_id, "G1-P1A", "G2P3B").await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn move_snapshot_slot_rejects_same_source_and_destination() {
    let app = build_bank_router();
    let detail = create_three_slot_snapshot(&app).await;
    let snap_id = detail.snapshot.snapshot_id.clone();

    let resp = post_move_slot(&app, &snap_id, "G1-P1A", "G1-P1A").await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn move_snapshot_slot_unknown_snapshot_returns_400() {
    let app = build_bank_router();
    let resp = post_move_slot(&app, "snap_does_not_exist", "G1-P1A", "G1-P1B").await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// Scan + import HTTP flows
// ---------------------------------------------------------------------------

use crate::formats::pat as pat_format;
use crate::formats::seq as seq_format;
use crate::library::model::FileIngestStatus;
use crate::pattern::Pattern;
use crate::step;
use crate::web::api_types::{
    ImportResponse, ScanJobResponse, ScanJobStatus, ScanResponse, ScanStartResponse,
};

fn build_bank_router_with(library: Arc<LibraryStore>) -> Router {
    build_bank_router_with_state(library).0
}

fn build_bank_router_with_state(library: Arc<LibraryStore>) -> (Router, Arc<AppState>) {
    let state = AppState::for_tests(
        ScratchSlot {
            patgroup: 0,
            slot: 0,
            side: 0,
        },
        library,
        String::new(),
        UiConfigSnapshot::for_tests(),
        std::path::PathBuf::from("TD3_CONFIG.env"),
    );
    let app = Router::new()
        .nest("/api", bank_handlers::router())
        .with_state(state.clone());
    (app, state)
}

fn make_pattern(note: u8) -> Pattern {
    let mut steps: [step::Step; 16] = Default::default();
    for s in steps.iter_mut() {
        s.note = note;
    }
    Pattern::new(false, 16, steps).unwrap()
}

fn make_pat_lossy_pattern() -> Pattern {
    let mut steps: [step::Step; 16] = Default::default();
    steps[0].note = 4;
    steps[0].time = step::Time::Normal;
    steps[1].note = 4;
    steps[1].time = step::Time::TieRest;
    Pattern::new(false, 16, steps).unwrap()
}

fn fresh_tmp_dir(tag: &str) -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("td3-bank-api-scan-{}-{}-{}", tag, pid, n));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

async fn post_scan(app: &Router, dir: &std::path::Path) -> (StatusCode, ScanStartResponse) {
    let body = serde_json::to_vec(&serde_json::json!({
        "path": dir.to_string_lossy(),
        "recursive": true,
    }))
    .unwrap();
    let req = Request::builder()
        .method("POST")
        .uri("/api/bank/scan")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let payload: ScanStartResponse = serde_json::from_slice(&bytes).unwrap();
    (status, payload)
}

async fn get_scan_job(app: &Router, job_id: &str) -> (StatusCode, ScanJobResponse) {
    let req = Request::builder()
        .uri(format!("/api/bank/scan/{}", job_id))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let payload: ScanJobResponse = serde_json::from_slice(&bytes).unwrap();
    (status, payload)
}

async fn wait_for_scan_job(app: &Router, job_id: &str) -> ScanJobResponse {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut last_payload: Option<ScanJobResponse> = None;
    while std::time::Instant::now() < deadline {
        let (status, payload) = get_scan_job(app, job_id).await;
        assert_eq!(status, StatusCode::OK);
        if matches!(
            payload.status,
            ScanJobStatus::Completed | ScanJobStatus::Failed | ScanJobStatus::Cancelled
        ) {
            return payload;
        }
        last_payload = Some(payload);
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    if let Some(payload) = last_payload {
        panic!(
            "scan job did not finish within 30s; last status: {:?}, found: {}, parsed: {}, error: {}",
            payload.status,
            payload.found,
            payload.parsed,
            payload.error.as_deref().unwrap_or("<none>")
        );
    }
    panic!("scan job did not finish within 30s; no status response was observed");
}

#[test]
fn scan_job_registry_status_transitions() {
    let registry = crate::web::scan_jobs::ScanJobRegistry::new();
    let start = registry.start("C:/scan-root".to_string()).unwrap();
    assert_eq!(start.status, ScanJobStatus::Queued);

    registry.mark_running(&start.job_id);
    let running = registry.get(&start.job_id).unwrap();
    assert_eq!(running.status, ScanJobStatus::Running);

    registry.complete(
        &start.job_id,
        ScanResponse {
            batch_id: "batch_1".to_string(),
            entries: Vec::new(),
        },
    );
    let completed = registry.get(&start.job_id).unwrap();
    assert_eq!(completed.status, ScanJobStatus::Completed);
    assert_eq!(completed.batch_id.as_deref(), Some("batch_1"));
    assert!(completed.finished_at_epoch_ms.is_some());
}

#[tokio::test]
async fn scan_endpoint_creates_batch_and_entries() {
    let library = temp_library();
    let app = build_bank_router_with(library.clone());

    let dir = fresh_tmp_dir("scan");
    // Lay down one real .seq and one unknown file. The scan endpoint now
    // batches only supported candidate files, so the unknown file is ignored
    // during ingest and does not surface as an Unsupported entry.
    let p1 = dir.join("one.seq");
    let bytes = seq_format::export(&make_pattern(0)).unwrap();
    std::fs::write(&p1, &bytes).unwrap();
    std::fs::write(dir.join("ignored.txt"), b"nope").unwrap();

    let (status, start) = post_scan(&app, &dir).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(start.status, ScanJobStatus::Queued);
    assert_eq!(start.found, 0);
    assert_eq!(start.parsed, 0);

    let job = wait_for_scan_job(&app, &start.job_id).await;
    assert_eq!(job.status, ScanJobStatus::Completed);
    assert!(job.batch_id.is_some());
    let payload = ScanResponse {
        batch_id: job.batch_id.clone().unwrap(),
        entries: job.entries.clone(),
    };
    assert_eq!(
        payload.entries.len(),
        1,
        "scan should return only supported candidate entries"
    );

    let imported = payload
        .entries
        .iter()
        .filter(|e| e.status == FileIngestStatus::Imported)
        .count();
    let unsupported = payload
        .entries
        .iter()
        .filter(|e| e.status == FileIngestStatus::Unsupported)
        .count();
    assert_eq!(imported, 1);
    assert_eq!(unsupported, 0);
    assert!(payload
        .entries
        .iter()
        .all(|entry| entry.path.ends_with("one.seq")));

    // Batch persisted with real counters.
    let batch = library
        .get_import_batch(&payload.batch_id)
        .unwrap()
        .unwrap();
    assert_eq!(batch.files_found, 1);
    assert_eq!(batch.files_imported, 1);
    assert_eq!(batch.unsupported, 0);
    assert!(batch.finished_at.is_some());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn scan_start_response_shape_is_pinned() {
    let library = temp_library();
    let app = build_bank_router_with(library);
    let dir = fresh_tmp_dir("shape");

    let body = serde_json::to_vec(&serde_json::json!({
        "path": dir.to_string_lossy(),
        "recursive": true,
    }))
    .unwrap();
    let req = Request::builder()
        .method("POST")
        .uri("/api/bank/scan")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert!(payload["job_id"].as_str().unwrap().starts_with("scan_"));
    assert_eq!(payload["status"], "queued");
    assert_eq!(payload["path"], dir.to_string_lossy().to_string());
    assert_eq!(payload["found"], 0);
    assert_eq!(payload["parsed"], 0);
    assert!(payload["started_at_epoch_ms"].as_u64().unwrap() > 0);

    let job_id = payload["job_id"].as_str().unwrap();
    let job = wait_for_scan_job(&app, job_id).await;
    assert_eq!(job.status, ScanJobStatus::Completed);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn scan_missing_path_is_visible_through_job_status() {
    let library = temp_library();
    let app = build_bank_router_with(library);
    let dir = fresh_tmp_dir("missing");
    std::fs::remove_dir_all(&dir).unwrap();

    let (status, start) = post_scan(&app, &dir).await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let job = wait_for_scan_job(&app, &start.job_id).await;
    assert_eq!(job.status, ScanJobStatus::Failed);
    assert!(job
        .error
        .as_deref()
        .unwrap()
        .contains("scan path does not exist"));
}

#[tokio::test]
async fn scan_rejects_concurrent_job() {
    let library = temp_library();
    let (app, state) = build_bank_router_with_state(library);
    let active = state.scan.jobs.start("active-scan".to_string()).unwrap();
    let dir = fresh_tmp_dir("concurrent");

    let body = serde_json::to_vec(&serde_json::json!({
        "path": dir.to_string_lossy(),
        "recursive": true,
    }))
    .unwrap();
    let req = Request::builder()
        .method("POST")
        .uri("/api/bank/scan")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(body["error"].as_str().unwrap().contains(&active.job_id));

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn import_endpoint_processes_explicit_paths() {
    let library = temp_library();
    let app = build_bank_router_with(library.clone());

    let dir = fresh_tmp_dir("import");
    let p1 = dir.join("a.seq");
    let p2 = dir.join("b.seq");
    std::fs::write(&p1, seq_format::export(&make_pattern(1)).unwrap()).unwrap();
    // Same payload bytes on disk as p1 → duplicate by content_hash.
    std::fs::write(&p2, seq_format::export(&make_pattern(1)).unwrap()).unwrap();

    let body = serde_json::to_vec(&serde_json::json!({
        "paths": [
            p1.to_string_lossy(),
            p2.to_string_lossy(),
        ]
    }))
    .unwrap();
    let req = Request::builder()
        .method("POST")
        .uri("/api/bank/import")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let payload: ImportResponse = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(payload.entries.len(), 2);

    let imported = payload
        .entries
        .iter()
        .filter(|e| e.status == FileIngestStatus::Imported)
        .count();
    let dups = payload
        .entries
        .iter()
        .filter(|e| e.status == FileIngestStatus::DuplicateSkipped)
        .count();
    assert_eq!(imported, 1);
    assert_eq!(dups, 1);

    let batch = library
        .get_import_batch(&payload.batch_id)
        .unwrap()
        .unwrap();
    assert_eq!(batch.files_imported, 1);
    assert_eq!(batch.duplicates_skipped, 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn import_endpoint_skips_pat_derived_from_native_truth() {
    let library = temp_library();
    let app = build_bank_router_with(library);

    let dir = fresh_tmp_dir("derived-pat");
    let pattern = make_pat_lossy_pattern();
    let seq_path = dir.join("G1P2A.seq");
    let pat_path = dir.join("G1P2A.pat");
    std::fs::write(&seq_path, seq_format::export(&pattern).unwrap()).unwrap();
    std::fs::write(&pat_path, pat_format::export(&pattern)).unwrap();

    let body = serde_json::to_vec(&serde_json::json!({
        "paths": [
            pat_path.to_string_lossy(),
            seq_path.to_string_lossy(),
        ]
    }))
    .unwrap();
    let req = Request::builder()
        .method("POST")
        .uri("/api/bank/import")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let payload: ImportResponse = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(payload.entries.len(), 2);

    let seq_entry = payload
        .entries
        .iter()
        .find(|entry| entry.path == seq_path.to_string_lossy())
        .expect("seq entry");
    let pat_entry = payload
        .entries
        .iter()
        .find(|entry| entry.path == pat_path.to_string_lossy())
        .expect("pat entry");

    assert_eq!(seq_entry.status, FileIngestStatus::Imported);
    assert_eq!(pat_entry.status, FileIngestStatus::DuplicateSkipped);
    assert_eq!(
        pat_entry.duplicate_of.as_deref(),
        seq_entry.item_id.as_deref()
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// Compare + duplicates endpoints
// ---------------------------------------------------------------------------

use crate::pattern::pattern_to_sysex;
use crate::web::api_types::{DuplicatesResponse, ItemCompareResponse};

fn seed_item_with_sidecar(
    library: &Arc<LibraryStore>,
    item_id: &str,
    display: &str,
    pattern: &Pattern,
) {
    use crate::library::model::{AnalysisStatus, DuplicateStatus, LibraryItem, SourceKind};
    let now = crate::library::store::now_iso();
    let item = LibraryItem {
        item_id: item_id.to_string(),
        display_name: display.to_string(),
        source_kind: SourceKind::File,
        source_label: display.to_string(),
        source_path: None,
        created_at: now.clone(),
        updated_at: now,
        tags: Vec::new(),
        favorite: false,
        archived: false,
        slot_key: None,
        snapshot_id: None,
        snapshot_name: None,
        format: Some("seq".to_string()),
        scale_name: None,
        root_note: None,
        duplicate_status: DuplicateStatus::Unknown,
        related_group_count: 0,
        analysis_status: AnalysisStatus::Unknown,
        notes: None,
        content_hash: None,
    };
    library.upsert_item(item).unwrap();
    let sx = pattern_to_sysex(pattern, 0, 0, 0).unwrap();
    library.write_pattern_bytes(item_id, &sx[3..]).unwrap();
}

fn pattern_from_notes(notes: &[u8]) -> Pattern {
    let mut steps: [step::Step; 16] = Default::default();
    for (i, &n) in notes.iter().enumerate().take(16) {
        steps[i] = step::Step {
            note: n,
            transpose: step::Transpose::Normal,
            accent: step::Accent::Off,
            slide: step::Slide::Off,
            time: step::Time::Normal,
        };
    }
    Pattern::new(false, 16, steps).unwrap()
}

#[tokio::test]
async fn compare_items_endpoint_returns_diffs() {
    let library = temp_library();
    let app = build_bank_router_with(library.clone());

    let a = pattern_from_notes(&[0, 2, 4, 5, 7, 9, 11, 12, 0, 2, 4, 5, 7, 9, 11, 12]);
    let mut notes_b = [0u8, 2, 4, 5, 7, 9, 11, 12, 0, 2, 4, 5, 7, 9, 11, 12];
    notes_b[4] = 6; // one note change
    let b = pattern_from_notes(&notes_b);
    seed_item_with_sidecar(&library, "cmp_a", "a", &a);
    seed_item_with_sidecar(&library, "cmp_b", "b", &b);

    let req = Request::builder()
        .uri("/api/bank/compare/items?a=cmp_a&b=cmp_b")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let payload: ItemCompareResponse = serde_json::from_slice(&body).unwrap();
    assert!(!payload.report.identical);
    assert_eq!(payload.report.note_diff, 1);
    assert_eq!(payload.report.differ_steps, vec![4]);
    assert!(payload.report.same_rhythm);
    assert!(payload.report.duplicate_score >= 0.7);

    // Missing sidecar → 400.
    let req = Request::builder()
        .uri("/api/bank/compare/items?a=cmp_a&b=unknown")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_item_pattern_returns_decoded_webpattern() {
    let library = temp_library();
    let app = build_bank_router_with(library.clone());

    let pat = pattern_from_notes(&[0, 2, 4, 5, 7, 9, 11, 12, 0, 2, 4, 5, 7, 9, 11, 12]);
    seed_item_with_sidecar(&library, "patt_a", "a", &pat);

    let req = Request::builder()
        .uri("/api/bank/items/patt_a/pattern")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let payload: crate::web::api_types::ItemPatternResponse =
        serde_json::from_slice(&body).unwrap();
    assert_eq!(payload.item_id, "patt_a");
    assert_eq!(payload.pattern.steps.len(), 16);
    assert_eq!(payload.pattern.active_steps, 16);
    assert!(!payload.pattern.triplet);
    let round_trip = payload
        .pattern
        .to_pattern()
        .expect("decoded WebPattern must round-trip back to Pattern");
    assert_eq!(round_trip.step[0].note, 0);
    assert_eq!(round_trip.step[2].note, 4);
}

#[tokio::test]
async fn get_item_pattern_missing_sidecar_returns_400() {
    let library = temp_library();
    let app = build_bank_router_with(library.clone());
    seed_basic_item(&library, "no_sidecar");

    let req = Request::builder()
        .uri("/api/bank/items/no_sidecar/pattern")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_item_pattern_unknown_id_returns_400() {
    let library = temp_library();
    let app = build_bank_router_with(library);
    let req = Request::builder()
        .uri("/api/bank/items/does_not_exist/pattern")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn duplicates_endpoint_returns_clusters() {
    let library = temp_library();
    let app = build_bank_router_with(library.clone());

    // Two byte-identical items → exact cluster.
    let pat = pattern_from_notes(&[0, 2, 4, 5, 7, 9, 11, 12]);
    seed_item_with_sidecar(&library, "dup_a", "a", &pat);
    seed_item_with_sidecar(&library, "dup_b", "b", &pat);
    // One unrelated item.
    let other = pattern_from_notes(&[1, 5, 7, 9, 11, 12, 6, 3, 8, 2, 4, 10, 0, 1, 2, 3]);
    seed_item_with_sidecar(&library, "solo", "solo", &other);

    let req = Request::builder()
        .uri("/api/bank/duplicates")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let payload: DuplicatesResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload.clusters.len(), 1);
    let mut ids = payload.clusters[0].item_ids.clone();
    ids.sort();
    assert_eq!(ids, vec!["dup_a".to_string(), "dup_b".to_string()]);
    assert!(!payload
        .clusters
        .iter()
        .any(|c| c.item_ids.iter().any(|i| i == "solo")));

    // Write-through: items in the cluster get ExactDuplicate status.
    let dup_a = library.get_item("dup_a").unwrap().unwrap();
    assert_eq!(
        dup_a.duplicate_status,
        crate::library::model::DuplicateStatus::ExactDuplicate
    );
    let solo = library.get_item("solo").unwrap().unwrap();
    assert_eq!(
        solo.duplicate_status,
        crate::library::model::DuplicateStatus::Unique
    );
}

// ---------------------------------------------------------------------------
// Related groups + merge plan operations endpoints
// ---------------------------------------------------------------------------

use crate::library::model::{
    AnalysisStatus, DuplicateStatus, LibraryItem, SnapshotOrigin, SourceKind,
};
use crate::web::api_types::{MergePlanResponse, RelatedGroupsResponse};

fn seed_item_meta(library: &Arc<LibraryStore>, id: &str, scale: Option<&str>, root: Option<&str>) {
    let now = crate::library::store::now_iso();
    let item = LibraryItem {
        item_id: id.to_string(),
        display_name: id.to_string(),
        source_kind: SourceKind::File,
        source_label: id.to_string(),
        source_path: None,
        created_at: now.clone(),
        updated_at: now,
        tags: Vec::new(),
        favorite: false,
        archived: false,
        slot_key: None,
        snapshot_id: None,
        snapshot_name: None,
        format: Some("seq".to_string()),
        scale_name: scale.map(|s| s.to_string()),
        root_note: root.map(|s| s.to_string()),
        duplicate_status: DuplicateStatus::Unknown,
        related_group_count: 0,
        analysis_status: AnalysisStatus::Unknown,
        notes: None,
        content_hash: None,
    };
    library.upsert_item(item).unwrap();
}

#[tokio::test]
async fn related_endpoint_returns_groups() {
    let library = temp_library();
    let app = build_bank_router_with(library.clone());

    seed_item_meta(&library, "rel_a", Some("phrygian"), Some("E"));
    seed_item_meta(&library, "rel_b", Some("phrygian"), Some("E"));
    seed_item_meta(&library, "rel_c", Some("dorian"), Some("D"));

    let req = Request::builder()
        .uri("/api/bank/related")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let payload: RelatedGroupsResponse = serde_json::from_slice(&body).unwrap();

    // Both SameScale and SameRoot should fire on the {rel_a, rel_b} pair.
    let scale_groups: Vec<_> = payload
        .groups
        .iter()
        .filter(|g| matches!(g.kind, crate::library::related::GroupKind::SameScale))
        .collect();
    let root_groups: Vec<_> = payload
        .groups
        .iter()
        .filter(|g| matches!(g.kind, crate::library::related::GroupKind::SameRoot))
        .collect();
    assert_eq!(scale_groups.len(), 1);
    assert_eq!(root_groups.len(), 1);
    let mut scale_ids = scale_groups[0].item_ids.clone();
    scale_ids.sort();
    assert_eq!(scale_ids, vec!["rel_a".to_string(), "rel_b".to_string()]);

    // Kind filter narrows to a single classifier.
    let req = Request::builder()
        .uri("/api/bank/related?kind=same-scale")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let payload: RelatedGroupsResponse = serde_json::from_slice(&body).unwrap();
    assert!(payload
        .groups
        .iter()
        .all(|g| matches!(g.kind, crate::library::related::GroupKind::SameScale)));
    assert_eq!(payload.groups.len(), 1);

    // Unknown kind → 400.
    let req = Request::builder()
        .uri("/api/bank/related?kind=bogus")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn merge_plan_endpoint_returns_operations_shape() {
    let library = temp_library();
    let app = build_bank_router_with(library.clone());

    // Two empty snapshots - every operations row should be skip_empty_source.
    let s1 = library
        .create_snapshot("MergeSrc".into(), None, SnapshotOrigin::Manual)
        .unwrap();
    let s2 = library
        .create_snapshot("MergeDst".into(), None, SnapshotOrigin::Manual)
        .unwrap();

    let body = serde_json::to_vec(&serde_json::json!({
        "source_snapshot_id": s1.snapshot_id,
        "target_snapshot_id": s2.snapshot_id,
        "selection": [],
    }))
    .unwrap();
    let req = Request::builder()
        .method("POST")
        .uri("/api/bank/merge-plan")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let payload: MergePlanResponse = serde_json::from_slice(&body).unwrap();

    assert!(!payload.preview);
    assert_eq!(payload.plan.operations.len(), 64);
    assert!(payload.plan.operations.iter().all(|o| matches!(
        o.action,
        crate::library::merge_plan::MergeOperationAction::SkipEmptySource
    )));
    assert!(payload
        .plan
        .operations
        .iter()
        .all(|o| o.reason == "source_empty_skipped"));

    // Preview endpoint mirrors the same shape but flips the preview flag.
    let body = serde_json::to_vec(&serde_json::json!({
        "source_snapshot_id": s1.snapshot_id,
        "target_snapshot_id": s2.snapshot_id,
        "selection": [],
    }))
    .unwrap();
    let req = Request::builder()
        .method("POST")
        .uri("/api/bank/merge-plan/preview")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let payload: MergePlanResponse = serde_json::from_slice(&body).unwrap();
    assert!(payload.preview);
    assert_eq!(payload.plan.operations.len(), 64);
}
