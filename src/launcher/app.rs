//! Eframe/egui launcher app surfacing scratch-pattern selection, MIDI
//! status, the CLI help reference, and a persist-to-env checkbox.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;

use super::midi_probe::{self, ProbeStatus};
use super::selection::SelectionState;
use crate::env_writer;

/// Result returned to the caller once the launcher window closes.
#[derive(Debug, Clone)]
pub struct LauncherChoice {
    pub scratch: String,
    pub persist: bool,
}

#[derive(Debug, Clone, Default)]
pub struct LauncherOutcome(pub Option<LauncherChoice>);

const PROBE_INTERVAL: Duration = Duration::from_secs(2);

const RED: egui::Color32 = egui::Color32::from_rgb(204, 41, 54);
const RED_HOVER: egui::Color32 = egui::Color32::from_rgb(230, 60, 70);
const WHITE: egui::Color32 = egui::Color32::WHITE;

pub struct LauncherApp {
    selection: SelectionState,
    persist: bool,
    show_help: bool,
    help_text: String,
    midi_substring: String,
    midi_strict: bool,
    midi_status: ProbeStatus,
    next_probe_at: Instant,
    outcome: Arc<Mutex<LauncherOutcome>>,
    env_path: PathBuf,
    closing: bool,
}

impl LauncherApp {
    pub fn new(
        initial: SelectionState,
        midi_substring: String,
        midi_strict: bool,
        help_text: String,
        outcome: Arc<Mutex<LauncherOutcome>>,
        env_path: PathBuf,
    ) -> Self {
        let initial_status = midi_probe::probe(&midi_substring, midi_strict);
        Self {
            selection: initial,
            persist: false,
            show_help: false,
            help_text,
            midi_substring,
            midi_strict,
            midi_status: initial_status,
            next_probe_at: Instant::now() + PROBE_INTERVAL,
            outcome,
            env_path,
            closing: false,
        }
    }

    fn maybe_reprobe(&mut self) {
        if Instant::now() >= self.next_probe_at {
            self.midi_status = midi_probe::probe(&self.midi_substring, self.midi_strict);
            self.next_probe_at = Instant::now() + PROBE_INTERVAL;
        }
    }

    fn finish(&mut self, choice: Option<LauncherChoice>) -> ! {
        store_outcome(&self.outcome, choice.clone());
        if let Some(c) = choice {
            if c.persist {
                if let Err(err) = persist_scratch(&self.env_path, &c.scratch) {
                    eprintln!(
                        "warning: could not persist scratch slot to {}: {}",
                        self.env_path.display(),
                        err
                    );
                }
            }
            spawn_control_child(&c.scratch);
        }
        std::process::exit(0);
    }
}

fn persist_scratch(env_path: &std::path::Path, scratch: &str) -> std::io::Result<()> {
    let mut updates = HashMap::new();
    updates.insert("UI_SCRATCH_PATTERN".to_string(), scratch.to_string());
    env_writer::apply_updates(env_path, &updates).map_err(|e| std::io::Error::other(e.to_string()))
}

fn spawn_control_child(scratch: &str) {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(err) => {
            eprintln!("error: cannot resolve current executable: {}", err);
            return;
        }
    };
    let mut cmd = Command::new(exe);
    cmd.arg("control")
        .arg("--scratch-pattern")
        .arg(scratch)
        .env("TD3_SKIP_SCRATCH_CONFIRM", "1")
        .env("TD3_AUTO_OPEN_BROWSER", "1");
    match cmd.spawn() {
        Ok(_child) => {
            eprintln!("Launching td3-control with scratch slot {}...", scratch);
        }
        Err(err) => {
            eprintln!("error: failed to spawn control process: {}", err);
        }
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
                "Save this selection to TD3_CONFIG.env (UI_SCRATCH_PATTERN)",
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
                    !self.closing,
                    egui::Button::new(
                        egui::RichText::new("START")
                            .color(WHITE)
                            .strong()
                            .size(15.0),
                    )
                    .fill(RED),
                );
                if start.clicked() {
                    let choice = LauncherChoice {
                        scratch: self.selection.label(),
                        persist: self.persist,
                    };
                    self.closing = true;
                    self.finish(Some(choice));
                }
            });
        });
    }
}

pub(crate) fn store_outcome(
    outcome: &Arc<Mutex<LauncherOutcome>>,
    choice: Option<LauncherChoice>,
) -> bool {
    if let Ok(mut guard) = outcome.lock() {
        *guard = LauncherOutcome(choice);
        true
    } else {
        false
    }
}

fn render_status_row(ui: &mut egui::Ui, status: &ProbeStatus) {
    ui.horizontal(|ui| {
        let (dot_color, label) = match status {
            ProbeStatus::Connected { port_name } => (
                egui::Color32::from_rgb(40, 180, 70),
                format!("TD-3 connected - {}", port_name),
            ),
            ProbeStatus::NotFound => (
                egui::Color32::from_rgb(140, 140, 140),
                "No TD-3 detected (offline mode)".into(),
            ),
            ProbeStatus::DriverError(msg) => (
                egui::Color32::from_rgb(220, 160, 30),
                format!("MIDI driver error: {}", msg),
            ),
        };
        let (rect, _resp) = ui.allocate_exact_size(egui::Vec2::splat(14.0), egui::Sense::hover());
        ui.painter().circle_filled(rect.center(), 6.0, dot_color);
        ui.label(egui::RichText::new(label).strong());
    });
}

fn render_selector(ui: &mut egui::Ui, sel: &mut SelectionState) {
    ui.label(egui::RichText::new("Scratch Pattern Slot").strong());
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label("Group:");
        for g in 1..=4u8 {
            slot_button(ui, &g.to_string(), sel.group == g, || sel.group = g);
        }
    });
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label("Pattern:");
        for p in 1..=8u8 {
            slot_button(ui, &p.to_string(), sel.pattern == p, || sel.pattern = p);
        }
    });
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label("Side:");
        slot_button(ui, "A", !sel.side_b, || sel.side_b = false);
        slot_button(ui, "B", sel.side_b, || sel.side_b = true);
    });
}

fn slot_button(ui: &mut egui::Ui, label: &str, selected: bool, mut on_click: impl FnMut()) {
    let text = if selected {
        egui::RichText::new(label).color(WHITE).strong().size(15.0)
    } else {
        egui::RichText::new(label).size(15.0)
    };
    let mut button = egui::Button::new(text).min_size(egui::Vec2::new(36.0, 28.0));
    if selected {
        button = button.fill(RED).stroke(egui::Stroke::new(1.0, RED_HOVER));
    }
    if ui.add(button).clicked() {
        on_click();
    }
}

fn render_warning(ui: &mut egui::Ui, slot_label: &str) {
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(255, 246, 220))
        .stroke(egui::Stroke::new(
            1.0,
            egui::Color32::from_rgb(220, 170, 30),
        ))
        .inner_margin(egui::Margin::same(8))
        .corner_radius(4)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("WARNING: SCRATCH PATTERN")
                    .strong()
                    .color(egui::Color32::from_rgb(140, 70, 0)),
            );
            ui.add_space(2.0);
            ui.label(format!(
                "Pattern slot {} will be used as the scratch buffer and WILL BE OVERWRITTEN \
                 during normal operation. A full device bank backup is created automatically \
                 before any write occurs, so the original contents can be restored later.",
                slot_label
            ));
        });
}
