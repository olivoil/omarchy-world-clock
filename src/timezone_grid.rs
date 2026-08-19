use std::sync::OnceLock;

const DATA: &[u8] = include_bytes!("../data/timezone-grid.bin");
const MAGIC: &[u8; 8] = b"OWCTZ1\0\0";

#[derive(Debug)]
struct Segment {
    end_column: u16,
    zone_id: u16,
}

#[derive(Debug)]
struct TimezoneGrid {
    cells_per_degree: u16,
    columns: u16,
    rows: u16,
    row_offsets: Vec<u32>,
    segments: Vec<Segment>,
    zones: Vec<String>,
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = self.offset.checked_add(count)?;
        let value = self.bytes.get(self.offset..end)?;
        self.offset = end;
        Some(value)
    }

    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
}

impl TimezoneGrid {
    fn parse(bytes: &[u8]) -> Option<Self> {
        let mut cursor = Cursor::new(bytes);
        if cursor.take(MAGIC.len())? != MAGIC {
            return None;
        }
        let cells_per_degree = cursor.u16()?;
        let columns = cursor.u16()?;
        let rows = cursor.u16()?;
        let zone_count = cursor.u16()?;
        let segment_count = cursor.u32()?;
        if cells_per_degree == 0
            || columns != 360_u16.checked_mul(cells_per_degree)?
            || rows != 180_u16.checked_mul(cells_per_degree)?
            || zone_count == 0
        {
            return None;
        }

        let row_offsets = (0..=rows)
            .map(|_| cursor.u32())
            .collect::<Option<Vec<_>>>()?;
        if row_offsets.first().copied() != Some(0)
            || row_offsets.last().copied() != Some(segment_count)
            || row_offsets.windows(2).any(|pair| pair[0] > pair[1])
        {
            return None;
        }

        let segments = (0..segment_count)
            .map(|_| {
                Some(Segment {
                    end_column: cursor.u16()?,
                    zone_id: cursor.u16()?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        if segments.iter().any(|segment| {
            segment.end_column == 0 || segment.end_column > columns || segment.zone_id >= zone_count
        }) {
            return None;
        }
        for offsets in row_offsets.windows(2) {
            let start = usize::try_from(offsets[0]).ok()?;
            let end = usize::try_from(offsets[1]).ok()?;
            let row = segments.get(start..end)?;
            if row.is_empty()
                || row.last().map(|segment| segment.end_column) != Some(columns)
                || row
                    .windows(2)
                    .any(|pair| pair[0].end_column >= pair[1].end_column)
            {
                return None;
            }
        }

        let zones = (0..zone_count)
            .map(|_| {
                let length = usize::from(cursor.u16()?);
                let value = std::str::from_utf8(cursor.take(length)?).ok()?;
                Some(value.to_string())
            })
            .collect::<Option<Vec<_>>>()?;
        if cursor.offset != bytes.len() || zones.first().is_none_or(|zone| !zone.is_empty()) {
            return None;
        }

        Some(Self {
            cells_per_degree,
            columns,
            rows,
            row_offsets,
            segments,
            zones,
        })
    }

    fn timezone_at(&self, latitude: f64, longitude: f64) -> Option<&str> {
        if !latitude.is_finite()
            || !longitude.is_finite()
            || !(-90.0..=90.0).contains(&latitude)
            || !(-180.0..=180.0).contains(&longitude)
        {
            return None;
        }

        let scale = f64::from(self.cells_per_degree);
        let row = (((90.0 - latitude) * scale).floor() as usize).min(usize::from(self.rows) - 1);
        let column = (((longitude + 180.0) * scale).floor() as u16).min(self.columns - 1);
        let start = usize::try_from(*self.row_offsets.get(row)?).ok()?;
        let end = usize::try_from(*self.row_offsets.get(row + 1)?).ok()?;
        let row_segments = self.segments.get(start..end)?;
        let segment = row_segments
            .binary_search_by(|segment| {
                if segment.end_column <= column {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                }
            })
            .unwrap_err();
        let zone_id = usize::from(row_segments.get(segment)?.zone_id);
        self.zones
            .get(zone_id)
            .filter(|zone| !zone.is_empty())
            .map(String::as_str)
    }
}

fn grid() -> Option<&'static TimezoneGrid> {
    static GRID: OnceLock<Option<TimezoneGrid>> = OnceLock::new();
    GRID.get_or_init(|| TimezoneGrid::parse(DATA)).as_ref()
}

pub fn timezone_at(latitude: f64, longitude: f64) -> Option<String> {
    grid()?.timezone_at(latitude, longitude).map(str::to_string)
}
