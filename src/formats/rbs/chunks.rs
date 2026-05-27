use std::convert::TryInto;

use crate::error::Td3Error;

use super::{CHUNK_303, CHUNK_303_PAYLOAD_LEN, DEVICES};

/// Scan for the two canonical `303 ` chunks.
pub(super) fn find_303_chunks(data: &[u8]) -> Result<[usize; DEVICES], Td3Error> {
    let mut hits: Vec<usize> = Vec::new();
    let mut i = 0usize;
    while i + 8 <= data.len() {
        if &data[i..i + 4] == CHUNK_303 {
            let size =
                u32::from_be_bytes(data[i + 4..i + 8].try_into().map_err(|_| {
                    Td3Error::FormatError(".rbs chunk-size slice failed".to_string())
                })?) as usize;
            if size == CHUNK_303_PAYLOAD_LEN && i + 8 + size <= data.len() {
                hits.push(i);
                i += 8 + size;
                continue;
            }
        }
        i += 1;
    }
    if hits.len() != DEVICES {
        return Err(Td3Error::FormatError(format!(
            ".rbs must contain exactly {} `303 ` chunks of {} bytes, found {}",
            DEVICES,
            CHUNK_303_PAYLOAD_LEN,
            hits.len()
        )));
    }
    Ok([hits[0], hits[1]])
}

/// Return the payload slice of a `303 ` chunk located at `chunk_off`.
pub(super) fn chunk_payload(data: &[u8], chunk_off: usize) -> Result<&[u8], Td3Error> {
    let start = chunk_off + 8;
    let end = start + CHUNK_303_PAYLOAD_LEN;
    if end > data.len() {
        return Err(Td3Error::FormatError(format!(
            ".rbs `303 ` chunk at {:#x} overflows file (needs {} bytes)",
            chunk_off, CHUNK_303_PAYLOAD_LEN
        )));
    }
    Ok(&data[start..end])
}
