mod app;
mod app_env;
mod bank;
mod browser;
mod config;
mod env_metadata;
mod env_writer;
mod error;
mod formats;
mod launcher;
mod library;
mod midi_exchange_lock;
mod midi_io;
mod midi_ports;
mod midi_session;
mod path_safety;
mod pattern;
mod step;
mod td3_protocol;
mod web;

#[cfg(test)]
mod tests;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .format_timestamp(None)
        .format_target(false)
        .init();

    let result = if is_bare_invocation() {
        run_launcher_path()
    } else {
        run_cli_path()
    };

    match result {
        Ok(_) => (),
        Err(err) => {
            // Single-instance guard exit codes:
            //   2 = control-mode port bind collision (another control UI running)
            //   3 = MIDI device busy (another td3-control holds the port)
            //   1 = everything else.
            let exit_code = match &err {
                error::Td3Error::InstanceRunning { .. } => 2,
                error::Td3Error::DeviceBusy { .. } => 3,
                _ => 1,
            };
            eprintln!("error: {}", err);
            std::process::exit(exit_code);
        }
    }
}

fn is_bare_invocation() -> bool {
    std::env::args().count() <= 1
}

fn run_cli_path() -> Result<(), error::Td3Error> {
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| error::Td3Error::Other(format!("tokio runtime init: {}", e)))?;
    rt.block_on(run())
}

fn run_launcher_path() -> Result<(), error::Td3Error> {
    let env_path = std::path::PathBuf::from(app_env::CONFIG_FILE_PATH);
    let (env, first_run) = app_env::AppEnv::load_or_create(&env_path)?;
    if first_run {
        eprintln!(
            "Created {} with default settings.",
            app_env::CONFIG_FILE_PATH
        );
        eprintln!("Edit this file to customize future startups.");
        eprintln!();
    }
    formats::mid::set_default_bpm(env.ui_default_bpm);

    // launcher::run normally never returns when the user clicks Start or
    // Cancel - the launcher app spawns a control-mode child process and
    // calls `std::process::exit(0)` directly. The only path back here is
    // the user closing the window via the title-bar X button, in which
    // case we exit cleanly without launching anything.
    match launcher::run(&env, env_path)? {
        Some(_) => Ok(()),
        None => Ok(()),
    }
}

async fn run() -> Result<(), error::Td3Error> {
    let (env, first_run) =
        app_env::AppEnv::load_or_create(std::path::Path::new(app_env::CONFIG_FILE_PATH))?;
    if first_run {
        eprintln!(
            "Created {} with default settings.",
            app_env::CONFIG_FILE_PATH
        );
        eprintln!("Edit this file to customize future startups.");
        eprintln!();
    }
    formats::mid::set_default_bpm(env.ui_default_bpm);
    let config = config::load_config(&env)?;
    match &config.mode {
        config::Mode::Control => {
            if should_run_startup_midi(&env) {
                // Pre-UI backup is mandatory when startup MIDI is enabled so
                // the scratch warning's backup promise is fulfilled on disk.
                // When no device is found we drop into offline mode and the
                // web server still starts without a device session.
                match app::try_pre_ui_backup(&config)? {
                    Some(_) => {
                        confirm_scratch_pattern(&config)?;
                        app::force_usb_sync(&config);
                    }
                    None => print_offline_banner(),
                }
            } else {
                print_startup_midi_disabled_banner();
            }
            let auto_open_browser = browser::auto_open_browser_requested();
            web::start_server(config, &env, auto_open_browser).await
        }
        _ => app::run(config),
    }
}

fn should_run_startup_midi(env: &app_env::AppEnv) -> bool {
    env.ui_auto_connect_to_midi
}

/// Display scratch pattern warning and require y/n confirmation before starting.
fn confirm_scratch_pattern(config: &config::Config) -> Result<(), error::Td3Error> {
    let scratch = config.control.scratch_slot.as_ref().ok_or_else(|| {
        error::Td3Error::CliError("control mode requires scratch pattern".to_string())
    })?;
    if std::env::var("TD3_SKIP_SCRATCH_CONFIRM").as_deref() == Ok("1") {
        eprintln!(
            "Scratch slot {} confirmed via launcher GUI.",
            scratch.label()
        );
        return Ok(());
    }
    eprintln!();
    eprintln!("  ╔══════════════════════════════════════════════════════════════╗");
    eprintln!("  ║                   WARNING: SCRATCH PATTERN                   ║");
    eprintln!("  ╠══════════════════════════════════════════════════════════════╣");
    eprintln!("  ║                                                              ║");
    eprintln!(
        "  ║  Pattern slot {:<6} will be used as the scratch buffer.     ║",
        scratch.label()
    );
    eprintln!("  ║  This pattern WILL BE OVERWRITTEN during normal operation.   ║");
    eprintln!("  ║                                                              ║");
    eprintln!("  ║  A full device bank backup will be created before any        ║");
    eprintln!("  ║  writes occur, so you can always restore it later.           ║");
    eprintln!("  ║                                                              ║");
    eprintln!("  ╚══════════════════════════════════════════════════════════════╝");
    eprintln!();
    eprint!(
        "  Continue with {} as scratch pattern? [y/N] ",
        scratch.label()
    );

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|error| error::Td3Error::Other(format!("failed to read input: {}", error)))?;

    match input.trim().to_lowercase().as_str() {
        "y" | "yes" => {
            eprintln!("  Confirmed. Starting control UI...");
            eprintln!();
            Ok(())
        }
        _ => {
            eprintln!("  Aborted.");
            std::process::exit(0);
        }
    }
}

fn print_offline_banner() {
    eprintln!();
    eprintln!("  ╔══════════════════════════════════════════════════════════════╗");
    eprintln!("  ║                       OFFLINE MODE                           ║");
    eprintln!("  ╠══════════════════════════════════════════════════════════════╣");
    eprintln!("  ║                                                              ║");
    eprintln!("  ║  No TD-3 detected on the configured MIDI port.               ║");
    eprintln!("  ║  Pattern editing, generators, library, snapshots, and        ║");
    eprintln!("  ║  file export remain fully usable. Listen, Push, and Pull     ║");
    eprintln!("  ║  stay disabled until a device is connected.                  ║");
    eprintln!("  ║                                                              ║");
    eprintln!("  ╚══════════════════════════════════════════════════════════════╝");
    eprintln!();
}

fn print_startup_midi_disabled_banner() {
    eprintln!();
    eprintln!("  ╔══════════════════════════════════════════════════════════════╗");
    eprintln!("  ║                    STARTUP MIDI DISABLED                     ║");
    eprintln!("  ╠══════════════════════════════════════════════════════════════╣");
    eprintln!("  ║                                                              ║");
    eprintln!("  ║  UI_AUTO_CONNECT_TO_MIDI=0, so startup MIDI probing and the  ║");
    eprintln!("  ║  pre-UI device backup are skipped. The web UI starts without ║");
    eprintln!("  ║  a device session. Device operations require a manual        ║");
    eprintln!("  ║  connection after the page opens.                            ║");
    eprintln!("  ║                                                              ║");
    eprintln!("  ╚══════════════════════════════════════════════════════════════╝");
    eprintln!();
}
