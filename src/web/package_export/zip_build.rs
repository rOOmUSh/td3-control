use super::*;

pub(super) fn build_zip_bytes(input: &PackageExportInput) -> Result<(Vec<u8>, u32), Td3Error> {
    let mut buf: Cursor<Vec<u8>> = Cursor::new(Vec::new());
    let mut file_count: u32 = 0;
    {
        let mut zip = ZipWriter::new(&mut buf);
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let midi_opts = input.midi_opts;

        if input.combined_rbs {
            let rbs_bytes =
                build_combined_rbs(input.acid_patterns, input.basslines, input.basslines_full)?;
            let name = format!("{}/combined.rbs", ROOT_FOLDER);
            zip_write(&mut zip, &name, &rbs_bytes, opts)?;
            file_count += 1;
        }
        if input.combined_sqs {
            let sqs_bytes =
                build_combined_sqs(input.acid_patterns, input.basslines, input.basslines_full)?;
            let name = format!("{}/combined.sqs", ROOT_FOLDER);
            zip_write(&mut zip, &name, &sqs_bytes, opts)?;
            file_count += 1;
        }

        for i in 0..4 {
            let pn = format!("P{}", i + 1);
            for fmt_id in input.formats {
                let (ext, data) = render_format(fmt_id, &input.acid_patterns[i], &pn, midi_opts)?;
                let name = format!("{}/{}/{}.{}", ROOT_FOLDER, pn, pn, ext);
                zip_write(&mut zip, &name, &data, opts)?;
                file_count += 1;
            }
            let pbn = format!("{}_BASSLINE", pn);
            for fmt_id in input.formats {
                let (ext, data) = render_format(fmt_id, &input.basslines[i], &pbn, midi_opts)?;
                let name = format!("{}/{}/{}/{}.{}", ROOT_FOLDER, pn, pbn, pbn, ext);
                zip_write(&mut zip, &name, &data, opts)?;
                file_count += 1;
            }
        }

        zip.finish()
            .map_err(|e| Td3Error::Other(format!("zip finalize: {}", e)))?;
    }
    Ok((buf.into_inner(), file_count))
}

fn zip_write<W: Write + std::io::Seek>(
    zip: &mut ZipWriter<W>,
    name: &str,
    data: &[u8],
    opts: SimpleFileOptions,
) -> Result<(), Td3Error> {
    zip.start_file(name, opts)
        .map_err(|e| Td3Error::Other(format!("zip start_file {}: {}", name, e)))?;
    zip.write_all(data)
        .map_err(|e| Td3Error::Other(format!("zip write {}: {}", name, e)))?;
    Ok(())
}
