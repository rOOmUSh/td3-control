use clap::{Parser, Subcommand};

use super::args::{
    ControlArgs, ConvertArgs, ExportArgs, ExtractBankArgs, ImportArgs, ImportBankArgs, PackBankArgs,
};

/// Behringer TD-3 MIDI Control Interface.
#[derive(Parser, Debug)]
#[command(name = "td3-control", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Export a pattern from TD-3
    Export(ExportArgs),
    /// Import a pattern file and import it to TD-3
    Import(ImportArgs),
    /// List available MIDI ports
    ListPorts,
    /// Start web control interface
    Control(ControlArgs),
    /// Convert a pattern file between formats (no device required)
    Convert(ConvertArgs),
    /// Extract a `.sqs` full-bank file into a folder of 64 per-pattern subfolders
    ExtractBank(ExtractBankArgs),
    /// Pack a 64-subfolder tree back into a `.sqs` full-bank file
    PackBank(PackBankArgs),
    /// Import a `.sqs` full-bank to TD-3 with mandatory atomic pre-write backup
    ImportBank(ImportBankArgs),
}
