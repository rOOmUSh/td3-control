//! Renders the clap-generated help string for display inside the launcher.

use clap::CommandFactory;

use crate::config::Cli;

pub fn full_help() -> String {
    Cli::command().render_help().to_string()
}
