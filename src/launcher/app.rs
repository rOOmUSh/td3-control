//! Eframe/egui launcher app surfacing scratch-pattern selection, MIDI
//! startup selection, the CLI help reference, and a persist-to-env checkbox.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;

use super::child_args::LauncherMidiChoice;
use super::choice::{store_outcome, LauncherChoice, LauncherOutcome};
use super::device_options;
use super::midi_probe::{self, PortListing, ProbeStatus};
use super::persist::{persist_launcher_choice, LauncherPersistChoice};
use super::process::spawn_control_child;
use super::selection::SelectionState;
use super::startup_state;
use super::view::{
    render_launch_error, render_midi_selection, render_selector, render_status_row, render_warning,
    render_web_port,
};
use super::web_port::{self, WebPortStatus};

const PROBE_INTERVAL: Duration = Duration::from_secs(2);

const RED: egui::Color32 = egui::Color32::from_rgb(204, 41, 54);
const WHITE: egui::Color32 = egui::Color32::WHITE;

pub struct LauncherApp {
    selection: SelectionState,
    persist: bool,
    show_help: bool,
    help_text: String,
    midi_substring: String,
    midi_strict: bool,
    midi_status: ProbeStatus,
    midi_ports: PortListing,
    midi_ports_error: Option<String>,
    selected_input_port: Option<String>,
    selected_output_port: Option<String>,
    midi_selection_message: Option<String>,
    web_bind: String,
    web_port_text: String,
    web_port_status: WebPortStatus,
    launch_error: Option<String>,
    next_probe_at: Instant,
    outcome: Arc<Mutex<LauncherOutcome>>,
    env_path: PathBuf,
    closing: bool,
}

pub struct LauncherAppConfig {
    pub initial: SelectionState,
    pub midi_substring: String,
    pub midi_strict: bool,
    pub web_port: u16,
    pub web_bind: String,
    pub help_text: String,
    pub outcome: Arc<Mutex<LauncherOutcome>>,
    pub env_path: PathBuf,
}

impl LauncherApp {
    pub fn new(config: LauncherAppConfig) -> Self {
        let LauncherAppConfig {
            initial,
            midi_substring,
            midi_strict,
            web_port,
            web_bind,
            help_text,
            outcome,
            env_path,
        } = config;
        let initial_status = midi_probe::probe(&midi_substring, midi_strict);
        let (midi_ports, midi_ports_error) = startup_state::load_port_listing();
        let input_selection = device_options::select_configured_port(
            &midi_ports.inputs,
            &midi_substring,
            midi_strict,
        );
        let output_selection = device_options::select_configured_port(
            &midi_ports.outputs,
            &midi_substring,
            midi_strict,
        );
        let midi_selection_message =
            startup_state::configured_selection_message(&input_selection, &output_selection);
        let selected_input_port = input_selection.selected_name().map(str::to_string);
        let selected_output_port = output_selection.selected_name().map(str::to_string);
        let web_port_status = web_port::first_available_web_port(&web_bind, web_port);
        let web_port_text = web_port_status.port().unwrap_or(web_port).to_string();
        Self {
            selection: initial,
            persist: false,
            show_help: false,
            help_text,
            midi_substring,
            midi_strict,
            midi_status: initial_status,
            midi_ports,
            midi_ports_error,
            selected_input_port,
            selected_output_port,
            midi_selection_message,
            web_bind,
            web_port_text,
            web_port_status,
            launch_error: None,
            next_probe_at: Instant::now() + PROBE_INTERVAL,
            outcome,
            env_path,
            closing: false,
        }
    }

    fn maybe_reprobe(&mut self) {
        if Instant::now() >= self.next_probe_at {
            self.midi_status = midi_probe::probe(&self.midi_substring, self.midi_strict);
            let (ports, error) = startup_state::load_port_listing();
            self.midi_ports = ports;
            self.midi_ports_error = error;
            self.next_probe_at = Instant::now() + PROBE_INTERVAL;
        }
    }

    fn finish(&mut self, choice: Option<LauncherChoice>) -> ! {
        store_outcome(&self.outcome, choice.clone());
        if let Some(c) = choice {
            if c.persist {
                let persist_choice = LauncherPersistChoice {
                    scratch: c.scratch.clone(),
                    web_port: c.web_port,
                    midi: c.midi.clone(),
                };
                match persist_launcher_choice(&self.env_path, &persist_choice) {
                    Ok(report) => {
                        if matches!(c.midi, LauncherMidiChoice::ExactPair { .. })
                            && !report.midi_persisted
                        {
                            eprintln!(
                                "warning: MIDI input/output names differ, so the MIDI selection was session-only."
                            );
                        }
                    }
                    Err(err) => {
                        eprintln!(
                            "warning: could not persist launcher settings to {}: {}",
                            self.env_path.display(),
                            err
                        );
                    }
                }
            }
            spawn_control_child(&c);
        }
        std::process::exit(0);
    }

    fn current_web_port(&mut self) -> Result<u16, String> {
        self.web_port_status = web_port::validate_web_port(&self.web_bind, &self.web_port_text);
        if self.web_port_status.is_available() {
            if let Some(port) = self.web_port_status.port() {
                return Ok(port);
            }
        }
        Err(self.web_port_status.message())
    }

    fn build_choice(&mut self) -> Result<LauncherChoice, String> {
        let midi = startup_state::current_midi_choice(
            &self.midi_ports,
            &self.selected_input_port,
            &self.selected_output_port,
        )?;
        let web_port = self.current_web_port()?;
        Ok(LauncherChoice {
            scratch: self.selection.label(),
            persist: self.persist,
            midi,
            web_port,
        })
    }

    fn can_start(&self) -> bool {
        startup_state::can_start(
            &self.web_port_status,
            &self.selected_input_port,
            &self.selected_output_port,
        )
    }
}

impl eframe::App for LauncherApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.maybe_reprobe();
        ctx.request_repaint_after(Duration::from_millis(500));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.closing {
            egui::CentralPanel::default().show_inside(ui, |_ui| {});
            return;
        }

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.heading("TD-3 Control Launcher");
            ui.add_space(6.0);

            render_status_row(ui, &self.midi_status);
            render_midi_selection(
                ui,
                &self.midi_ports,
                self.midi_ports_error.as_deref(),
                &self.midi_selection_message,
                &mut self.selected_input_port,
                &mut self.selected_output_port,
            );
            ui.separator();

            render_selector(ui, &mut self.selection);
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(format!("Selected scratch slot: {}", self.selection.label()))
                    .strong(),
            );
            ui.add_space(6.0);
            render_warning(ui, &self.selection.label());
            ui.add_space(8.0);

            ui.checkbox(
                &mut self.persist,
                "Save launcher settings to TD3_CONFIG.env",
            );
            if self.persist
                && startup_state::midi_selection_is_session_only(
                    &self.selected_input_port,
                    &self.selected_output_port,
                )
            {
                ui.colored_label(
                    egui::Color32::from_rgb(160, 90, 0),
                    "MIDI selection will be session-only because input and output names differ.",
                );
            }
            ui.add_space(6.0);
            render_web_port(
                ui,
                &self.web_bind,
                &mut self.web_port_text,
                &mut self.web_port_status,
            );
            ui.add_space(4.0);

            ui.collapsing("Show CLI Help", |ui| {
                self.show_help = true;
                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.help_text.as_str())
                                .desired_width(f32::INFINITY),
                        );
                    });
            });
            if !self.show_help {
                // ensure show_help stays consistent for tests / external observers
            }

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                let cancel_clicked = ui
                    .add_enabled(
                        !self.closing,
                        egui::Button::new(egui::RichText::new("Cancel").size(14.0)),
                    )
                    .clicked();
                if cancel_clicked {
                    self.closing = true;
                    self.finish(None);
                }
                ui.add_space(20.0);
                let start = ui.add_enabled(
                    !self.closing && self.can_start(),
                    egui::Button::new(
                        egui::RichText::new("START")
                            .color(WHITE)
                            .strong()
                            .size(15.0),
                    )
                    .fill(RED),
                );
                if start.clicked() {
                    match self.build_choice() {
                        Ok(choice) => {
                            self.closing = true;
                            self.finish(Some(choice));
                        }
                        Err(message) => {
                            self.launch_error = Some(message);
                        }
                    }
                }
            });
            render_launch_error(ui, self.launch_error.as_deref());
        });
    }
}
