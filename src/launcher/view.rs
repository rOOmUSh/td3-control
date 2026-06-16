//! Egui rendering helpers for the launcher window.

use eframe::egui;

use super::midi_probe::{PortListing, ProbeStatus};
use super::selection::SelectionState;
use super::web_port::{self, WebPortStatus};

const RED: egui::Color32 = egui::Color32::from_rgb(204, 41, 54);
const RED_HOVER: egui::Color32 = egui::Color32::from_rgb(230, 60, 70);
const WHITE: egui::Color32 = egui::Color32::WHITE;

pub(crate) fn render_status_row(ui: &mut egui::Ui, status: &ProbeStatus) {
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

pub(crate) fn render_midi_selection(
    ui: &mut egui::Ui,
    ports: &PortListing,
    error: Option<&str>,
    selection_message: &Option<String>,
    selected_input: &mut Option<String>,
    selected_output: &mut Option<String>,
) {
    ui.add_space(6.0);
    ui.label(egui::RichText::new("MIDI Device Ports").strong());
    if let Some(message) = selection_message {
        ui.label(message);
    }
    if let Some(error) = error {
        ui.colored_label(
            egui::Color32::from_rgb(160, 90, 0),
            format!("MIDI port list error: {}", error),
        );
    }
    ui.horizontal(|ui| {
        render_port_combo(ui, "Input", &ports.inputs, selected_input);
        render_port_combo(ui, "Output", &ports.outputs, selected_output);
    });
    if selected_input.is_some() != selected_output.is_some() {
        ui.colored_label(
            egui::Color32::from_rgb(160, 90, 0),
            "Select both MIDI input and output, or use TD3_CONFIG.env for both.",
        );
    }
}

pub(crate) fn render_web_port(
    ui: &mut egui::Ui,
    bind: &str,
    port_text: &mut String,
    status: &mut WebPortStatus,
) {
    ui.label(egui::RichText::new("Web UI Port").strong());
    ui.horizontal(|ui| {
        ui.label(format!("Bind: {}", bind));
        let response = ui.text_edit_singleline(port_text);
        if response.changed() {
            *status = web_port::validate_web_port(bind, port_text);
        }
    });
    let color = if status.is_available() {
        egui::Color32::from_rgb(40, 120, 60)
    } else {
        egui::Color32::from_rgb(160, 60, 40)
    };
    ui.colored_label(color, status.message());
}

pub(crate) fn render_launch_error(ui: &mut egui::Ui, message: Option<&str>) {
    if let Some(message) = message {
        ui.colored_label(egui::Color32::from_rgb(180, 40, 40), message);
    }
}

pub(crate) fn render_selector(ui: &mut egui::Ui, sel: &mut SelectionState) {
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

pub(crate) fn render_warning(ui: &mut egui::Ui, slot_label: &str) {
    let warning = [
        "Pattern slot ",
        slot_label,
        " will be used as the scratch buffer and WILL BE OVERWRITTEN \
         during normal operation. A full device bank backup is created automatically \
         before any write occurs, so the original contents can be restored later.",
    ]
    .concat();
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
            ui.label(warning);
        });
}

fn render_port_combo(
    ui: &mut egui::Ui,
    label: &str,
    ports: &[String],
    selected: &mut Option<String>,
) {
    let selected_text = selected
        .as_deref()
        .unwrap_or("Use TD3_CONFIG.env")
        .to_string();
    egui::ComboBox::from_label(label)
        .selected_text(selected_text)
        .width(260.0)
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(selected.is_none(), "Use TD3_CONFIG.env")
                .clicked()
            {
                *selected = None;
            }
            for port in ports {
                ui.selectable_value(selected, Some(port.clone()), port);
            }
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
