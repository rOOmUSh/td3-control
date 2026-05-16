use super::super::*;

/// Open `zip_path`, locate the `bank.sqs` entry, parse it, and return one
/// `(dashed_slot_key, 112-byte payload)` pair per record.
pub(in crate::library::store::snapshots) fn read_bank_sqs_payloads(
    zip_path: &Path,
) -> Option<Vec<(String, Vec<u8>)>> {
    use std::io::Read;
    let file = std::fs::File::open(zip_path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut f = archive.by_name("bank.sqs").ok()?;
        f.read_to_end(&mut buf).ok()?;
    }
    let bank = crate::formats::sqs::parse_bank(&buf).ok()?;
    let mut out: Vec<(String, Vec<u8>)> = Vec::with_capacity(bank.records.len());
    for rec in bank.records {
        let slot_num = rec.slot_addr & 0x7;
        let side = rec.slot_addr >> 3;
        let key = format!(
            "G{}-P{}{}",
            rec.group + 1,
            slot_num + 1,
            if side == 0 { 'A' } else { 'B' }
        );
        out.push((key, rec.payload));
    }
    Some(out)
}

/// Open `zip_path` and return the set of slot-folder names present in the
/// archive, such as `"G1P1A"` or `"G4P8B"`.
pub(in crate::library::store::snapshots) fn read_slot_presence(
    zip_path: &Path,
) -> Result<BTreeSet<String>, Td3Error> {
    let file = std::fs::File::open(zip_path)
        .map_err(|e| Td3Error::Other(format!("open zip {}: {}", zip_path.display(), e)))?;
    let archive = zip::ZipArchive::new(file)
        .map_err(|e| Td3Error::Other(format!("read zip {}: {}", zip_path.display(), e)))?;

    let mut present: BTreeSet<String> = BTreeSet::new();
    for name in archive.file_names() {
        if let Some(slot) = slot_folder_from_zip_name(name) {
            present.insert(slot);
        }
    }
    Ok(present)
}

fn slot_folder_from_zip_name(name: &str) -> Option<String> {
    let first = name.split(['/', '\\']).next()?;
    if is_slot_folder(first) {
        Some(first.to_string())
    } else {
        None
    }
}

fn is_slot_folder(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() < 5 || bytes.len() > 6 {
        return false;
    }
    if bytes[0] != b'G' {
        return false;
    }
    let mut i = 1usize;
    if i >= bytes.len() || !bytes[i].is_ascii_digit() {
        return false;
    }
    i += 1;
    if i >= bytes.len() || bytes[i] != b'P' {
        return false;
    }
    i += 1;
    if i >= bytes.len() || !bytes[i].is_ascii_digit() {
        return false;
    }
    i += 1;
    if i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i != bytes.len() - 1 {
        return false;
    }
    matches!(bytes[i], b'A' | b'B')
}
