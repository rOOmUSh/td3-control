use super::*;

mod analysis;
mod common;
mod file_index;
mod import_batches;
mod items;
mod snapshots;
mod tags;

#[allow(unused_imports)]
pub(in crate::library::persistence) use analysis::*;
#[allow(unused_imports)]
pub(in crate::library::persistence) use common::*;
#[allow(unused_imports)]
pub(in crate::library::persistence) use file_index::*;
#[allow(unused_imports)]
pub(in crate::library::persistence) use import_batches::*;
#[allow(unused_imports)]
pub(in crate::library::persistence) use items::*;
#[allow(unused_imports)]
pub(in crate::library::persistence) use snapshots::*;
#[allow(unused_imports)]
pub(in crate::library::persistence) use tags::*;

pub(in crate::library::persistence) fn save_data(
    conn: &Connection,
    data: &LibraryData,
) -> Result<(), Td3Error> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| Td3Error::Other(format!("library: begin sqlite transaction: {}", e)))?;

    write_format_version(&tx)?;
    write_items(&tx, &data.items)?;
    write_snapshots(&tx, &data.snapshots)?;
    write_snapshot_slots(&tx, &data.snapshot_slots)?;
    write_tags(&tx, &data.tags)?;
    write_item_tags(&tx, &data.item_tags)?;
    write_file_index(&tx, &data.file_index)?;
    write_pattern_analysis(&tx, &data.pattern_analysis)?;
    write_pattern_relations(&tx, &data.pattern_relations)?;
    write_import_batches(&tx, &data.import_batches)?;

    tx.commit()
        .map_err(|e| Td3Error::Other(format!("library: commit sqlite transaction: {}", e)))?;
    Ok(())
}
