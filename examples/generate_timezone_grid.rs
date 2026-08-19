use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use tzf_rs::Finder;

const MAGIC: &[u8; 8] = b"OWCTZ1\0\0";
const CELLS_PER_DEGREE: u16 = 10;
const COLUMNS: u16 = 360 * CELLS_PER_DEGREE;
const ROWS: u16 = 180 * CELLS_PER_DEGREE;

fn write_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn write_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn main() -> Result<()> {
    let output_path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("data/timezone-grid.bin"));
    let finder = Finder::new();
    let mut zone_ids = BTreeMap::<String, u16>::from([(String::new(), 0)]);
    let mut zones = vec![String::new()];
    let mut cells = Vec::with_capacity(usize::from(COLUMNS) * usize::from(ROWS));

    for row in 0..ROWS {
        let latitude = 90.0 - (f64::from(row) + 0.5) / f64::from(CELLS_PER_DEGREE);
        for column in 0..COLUMNS {
            let longitude = -180.0 + (f64::from(column) + 0.5) / f64::from(CELLS_PER_DEGREE);
            let found = finder.get_tz_name(longitude, latitude);
            let timezone = if found.is_empty() || found.starts_with("Etc/") {
                ""
            } else {
                found
            };
            let zone_id = if let Some(zone_id) = zone_ids.get(timezone) {
                *zone_id
            } else {
                let zone_id = u16::try_from(zones.len()).context("too many timezone names")?;
                zones.push(timezone.to_string());
                zone_ids.insert(timezone.to_string(), zone_id);
                zone_id
            };
            cells.push(zone_id);
        }
    }

    let mut row_offsets = Vec::<u32>::with_capacity(usize::from(ROWS) + 1);
    let mut segments = Vec::<(u16, u16)>::new();
    for row in 0..ROWS {
        row_offsets.push(u32::try_from(segments.len()).context("too many grid segments")?);
        let start = usize::from(row) * usize::from(COLUMNS);
        let row_cells = &cells[start..start + usize::from(COLUMNS)];
        let mut zone_id = row_cells[0];
        for column in 1..=COLUMNS {
            let next = (column < COLUMNS).then(|| row_cells[usize::from(column)]);
            if next != Some(zone_id) {
                segments.push((column, zone_id));
                if let Some(next) = next {
                    zone_id = next;
                }
            }
        }
    }
    row_offsets.push(u32::try_from(segments.len()).context("too many grid segments")?);

    if zones.len() > usize::from(u16::MAX) {
        bail!("too many timezone names");
    }

    let mut output = Vec::new();
    output.extend_from_slice(MAGIC);
    write_u16(&mut output, CELLS_PER_DEGREE);
    write_u16(&mut output, COLUMNS);
    write_u16(&mut output, ROWS);
    write_u16(
        &mut output,
        u16::try_from(zones.len()).context("too many timezone names")?,
    );
    write_u32(
        &mut output,
        u32::try_from(segments.len()).context("too many grid segments")?,
    );
    for offset in row_offsets {
        write_u32(&mut output, offset);
    }
    for (end_column, zone_id) in segments {
        write_u16(&mut output, end_column);
        write_u16(&mut output, zone_id);
    }
    for timezone in zones {
        let bytes = timezone.as_bytes();
        write_u16(
            &mut output,
            u16::try_from(bytes.len()).context("timezone name is too long")?,
        );
        output.extend_from_slice(bytes);
    }

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    fs::write(&output_path, &output).with_context(|| format!("write {}", output_path.display()))?;
    eprintln!(
        "wrote {} bytes to {} using {} zones",
        output.len(),
        output_path.display(),
        zone_ids.len()
    );
    Ok(())
}
