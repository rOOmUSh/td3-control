use std::fs::{self, File};
use std::io::{Cursor, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use zip::write::{SimpleFileOptions, ZipWriter};
use zip::CompressionMethod;

use crate::error::Td3Error;
use crate::formats::mid::MidiExportOptions;
use crate::formats::sqs::{
    serialize_bank, Bank, BankRecord, PAYLOAD_LEN, PRODUCT_UTF16BE, RECORD_COUNT, VERSION_UTF16BE,
};
use crate::formats::{self, rbs};
use crate::pattern::{pattern_to_sysex, Pattern};
use crate::step::{Step, Time};

mod combined_rbs;
mod combined_sqs;
mod export;
mod render;
mod time;
mod types;
mod zip_build;

use combined_rbs::{build_combined_rbs, silent_pattern};
use combined_sqs::build_combined_sqs;
use render::{clone_pattern, render_format};
use time::{timestamp_compact, timestamp_iso};
use zip_build::build_zip_bytes;

pub use export::export_package;
pub use time::sanitize_component;
pub use types::{PackageExportInput, PackageExportResult, ROOT_FOLDER};
