//! Web server for the TD-3 Control UI.
//!
//! Serves the frontend from `ui/` and exposes a JSON API
//! for MIDI device control.

pub(crate) mod api_types;
#[path = "bank/mod.rs"]
pub(crate) mod bank_handlers;
pub(crate) mod clock;
pub(crate) mod config_storage;
pub(crate) mod control_queue;
pub(crate) mod embedded_ui;
pub(crate) mod folder_picker;
pub(crate) mod handlers;
pub(crate) mod package_export;
pub(crate) mod remote_sync;
pub(crate) mod scan_jobs;
pub(crate) mod snapshot_export;
pub(crate) mod start_schedule;
pub(crate) mod state;
pub(crate) mod static_html;
pub(crate) mod user_config;

use std::net::SocketAddr;

use axum::routing::{get, post};
use axum::Router;

use crate::app_env::AppEnv;
use crate::config::Config;
use crate::error::Td3Error;
use crate::library::store::LibraryStore;
use crate::midi_session::{establish_td3_midi_session, Td3MidiSessionConfig};
use crate::td3_protocol;

/// Start the web server and block until shutdown.
///
/// `env` supplies the configuration file-derived fields that don't live on
/// `Config` (library DB path, pattern sidecar dir, UI boot snapshot). `Config`
/// still drives all of the "what are we doing this run" fields (scratch slot,
/// MIDI port names, bind address, etc.).
pub async fn start_server(config: Config, env: &AppEnv) -> Result<(), Td3Error> {
    let addr: SocketAddr = format!(
        "{}:{}",
        config.control.bind_address, config.control.listen_port
    )
    .parse()
    .map_err(|e| Td3Error::CliError(format!("invalid bind address: {}", e)))?;

    let scratch = config
        .control
        .scratch_slot
        .ok_or_else(|| Td3Error::CliError("control mode requires scratch pattern".to_string()))?;

    // Load the Bank Management library catalog at startup. Creation is
    // idempotent - a fresh catalog is materialized to disk if none exists.
    // Both the DB path and the per-item sidecar directory come from the env
    // file so operators can point them at an external drive.
    let library = std::sync::Arc::new(
        LibraryStore::load_or_create_with_sidecar(
            &env.library_database_path,
            &env.pattern_sidecar_dir,
        )
        .map_err(|e| Td3Error::Other(format!("library init failed: {}", e)))?,
    );

    let ui_config = state::UiConfigSnapshot {
        ui_auto_connect_to_midi: env.ui_auto_connect_to_midi,
        ui_auto_set_live_update: env.ui_auto_set_live_update,
        ui_default_bpm: env.ui_default_bpm,
        ui_default_triplet: env.ui_default_triplet,
        ui_max_bank_history_size: env.ui_max_bank_history_size,
        ui_rand_default_root: env.ui_rand_default_root,
        ui_rand_default_scale: env.ui_rand_default_scale.clone(),
        ui_rand_note_percent: env.ui_rand_note_percent,
        ui_rand_slide_percent: env.ui_rand_slide_percent,
        ui_rand_acc_percent: env.ui_rand_acc_percent,
        ui_rand_ud_percent: env.ui_rand_ud_percent,
        progression_next_pattern_save_step: env.progression_next_pattern_save_step,
    };

    // Prefer the resolved control-session backup dir (already layers
    // CLI > env > template) over the raw env value, so `--backup-dir`
    // wins for the bank-UI sync-backups fallback too.
    let backup_dir_path = config
        .control
        .backup_dir
        .clone()
        .unwrap_or_else(|| env.backup_dir_path.clone());

    // Resolved MIDI runtime config - precedence is CLI flag > env file >
    // template, all already merged into `Config`/`env` by this point.
    let midi_runtime = state::MidiRuntimeConfig {
        port_substring: env.midi_port_substring.clone(),
        strict_name_match: env.midi_strict_name_match,
        timeout: config.midi.request_timeout,
    };
    let midi_export_options = crate::formats::mid::MidiExportOptions::from_env(env);
    let midi_import_options = crate::formats::mid_import::MidiImportOptions::from_env(env);

    let shared_state = state::AppState::new(
        state::ScratchSlot {
            patgroup: scratch.patgroup,
            slot: scratch.slot,
            side: scratch.side,
        },
        library,
        backup_dir_path,
        state::AppConfigBundle {
            ui_config,
            env_file_path: std::path::PathBuf::from(crate::app_env::CONFIG_FILE_PATH),
            user_config_dir: std::path::PathBuf::from("config"),
        },
        state::MidiConfigBundle {
            runtime: midi_runtime,
            export_options: midi_export_options,
            import_options: midi_import_options,
        },
    );

    // Auto-connect to TD-3 on startup (best-effort, non-fatal)
    auto_connect(&shared_state, &config).await;

    let api = Router::new()
        .route("/status", get(handlers::status))
        .route("/ports", get(handlers::ports))
        .route("/midi/connect", post(handlers::connect))
        .route("/midi/disconnect", post(handlers::disconnect))
        .route("/midi/sync-source", post(handlers::set_sync_source))
        .route("/pattern/load", post(handlers::pattern_load))
        .route("/pattern/save", post(handlers::pattern_save))
        .route("/pattern/import", post(handlers::pattern_import))
        .route("/pattern/parse-bank", post(handlers::pattern_parse_bank))
        .route(
            "/pattern/play-preview",
            post(handlers::pattern_play_preview),
        )
        .route("/pattern/audition", post(handlers::pattern_audition))
        .route(
            "/pattern/audition/update",
            post(handlers::pattern_audition_update),
        )
        .route(
            "/pattern/audition/stop",
            post(handlers::pattern_audition_stop),
        )
        .route("/pattern/export-pool", post(handlers::export_pool))
        .route("/pattern/export", post(handlers::pattern_export))
        .route("/transport/start", post(handlers::transport_start))
        .route("/transport/stop", post(handlers::transport_stop))
        .route("/transport/bpm", post(handlers::transport_bpm))
        .route(
            "/transport/wrap-pulse",
            post(handlers::transport_wrap_pulse),
        )
        .route("/note/preview", post(handlers::note_preview))
        .route("/config/keyboard", get(handlers::get_keyboard_config))
        .route("/config/keyboard", post(handlers::save_keyboard_config))
        .route("/config/scales", get(handlers::get_scales_config))
        .route("/config/scales", post(handlers::save_scales_config))
        .route("/config/progression", get(handlers::get_progression_config))
        .route(
            "/config/progression",
            post(handlers::save_progression_config),
        )
        .route("/config/env", get(handlers::get_env_config))
        .route("/config/env/full", get(handlers::get_env_config_full))
        .route("/config/env", post(handlers::save_env_config))
        .route(
            "/config/env/reset-section",
            post(handlers::reset_env_config_section),
        )
        .route(
            "/progression/export-package",
            post(handlers::export_progression_package),
        )
        .route("/scratch-pattern", get(handlers::scratch_pattern))
        .merge(bank_handlers::router())
        .merge(control_queue::router())
        .merge(remote_sync::router());

    let app = Router::new()
        .nest("/api", api)
        // HTML pages: serve through the inject-config handler so the
        // browser sees `window.TD3_CONFIG_ENV` at first paint and never
        // needs to fetch `/api/config/env` to boot. Static assets (JS,
        // CSS, JSON) keep going through ServeDir below.
        .route("/", get(static_html::serve_index))
        .route("/index.html", get(static_html::serve_index))
        .route("/progression.html", get(static_html::serve_progression))
        .route("/bank.html", get(static_html::serve_bank))
        .route("/settings.html", get(static_html::serve_settings))
        .fallback(embedded_ui::serve_asset)
        .with_state(shared_state);

    eprintln!("TD-3 Control UI: http://{}", addr);
    eprintln!("Press Ctrl+C to stop");

    let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::AddrInUse {
            Td3Error::InstanceRunning {
                bind: config.control.bind_address.clone(),
                port: config.control.listen_port,
            }
        } else {
            Td3Error::Other(format!("failed to bind {}: {}", addr, e))
        }
    })?;

    axum::serve(listener, app)
        .await
        .map_err(|e| Td3Error::Other(format!("server error: {}", e)))?;

    Ok(())
}

/// Best-effort auto-connect to the TD-3 at startup.
/// If no device is found or the connection fails, the server starts without
/// a session and the user can connect manually from the UI.
async fn auto_connect(shared_state: &std::sync::Arc<state::AppState>, config: &Config) {
    let in_port_name = config.midi.input_port_name.clone();
    let out_port_name = config.midi.output_port_name.clone();
    let strict = config.midi.strict_name_match;
    let probe_timeout = config.midi.request_timeout;
    let result = tokio::task::block_in_place(move || {
        establish_td3_midi_session(Td3MidiSessionConfig {
            input_port_name: &in_port_name,
            output_port_name: &out_port_name,
            strict_name_match: strict,
            timeout: probe_timeout,
            sync_source_policy: td3_protocol::SyncSourceFailurePolicy::DefaultToUsb,
        })
    });

    match result {
        Ok(established) => {
            if let Some(err) = &established.info.sync_source_error {
                log::warn!(
                    "read sync source failed during auto-connect, defaulting to USB: {}",
                    err
                );
            }
            eprintln!(
                "Auto-connected: {} v{}",
                established.info.product_name, established.info.firmware_version
            );
            let mut guard = shared_state.midi.session.lock().await;
            *guard = Some(state::MidiSession {
                out_conn: Some(established.out_conn),
                rx: established.rx,
                _in_conn: established.in_conn,
                product_name: established.info.product_name,
                firmware_version: established.info.firmware_version,
                sync_source: established.info.sync_source,
            });
        }
        Err(e) => {
            eprintln!("Auto-connect failed (device not found): {}", e);
            eprintln!("You can connect manually from the web UI.");
        }
    }
}
