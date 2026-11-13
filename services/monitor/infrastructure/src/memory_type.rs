//! The machine's memory type from the `SMBIOS` memory device structures (type 17), so a `DDR4` PC labels its memory
//! line "RAM" with no driver installed.

use monitor_domain::MemoryKind;

const HEADER_LEN: usize = 8;
const MEMORY_DEVICE: u8 = 17;
const END_OF_TABLE: u8 = 127;
/// Offsets inside a memory device structure.
const SIZE_AT: usize = 0x0C;
const TYPE_AT: usize = 0x12;
const DDR5: u8 = 0x22;

/// The memory type byte of each populated module in a raw `SMBIOS` table. A truncated table ends the walk.
#[must_use]
pub fn memory_types(raw: &[u8]) -> Vec<u8> {
    let mut types = Vec::new();
    let mut rest = raw.get(HEADER_LEN..).unwrap_or_default();
    while let [kind, length, ..] = *rest {
        let length = usize::from(length);
        let Some(formatted) = rest.get(..length).filter(|_| length >= 4) else {
            break;
        };
        if kind == END_OF_TABLE {
            break;
        }
        if kind == MEMORY_DEVICE
            && let (Some(&[size_low, size_high]), Some(&memory_type)) = (
                formatted
                    .get(SIZE_AT..SIZE_AT + 2)
                    .and_then(|s| <&[u8; 2]>::try_from(s).ok()),
                formatted.get(TYPE_AT),
            )
            && u16::from_le_bytes([size_low, size_high]) != 0
        {
            types.push(memory_type);
        }
        // The string set ends at the first double NUL after the formatted part.
        let Some(strings) = rest.get(length..) else { break };
        let Some(end) = strings.windows(2).position(|pair| pair == [0, 0]) else {
            break;
        };
        rest = strings.get(end + 2..).unwrap_or_default();
    }
    types
}

/// `Ddr5` when any module is `DDR5`; with no modules listed the spec's default stands.
#[must_use]
pub fn memory_kind(types: &[u8]) -> MemoryKind {
    if types.is_empty() || types.contains(&DDR5) {
        MemoryKind::Ddr5
    } else {
        MemoryKind::Other
    }
}

/// The types with their counts, like "DDR5 x2".
#[must_use]
pub fn memory_text(types: &[u8]) -> String {
    let mut counts: Vec<(u8, usize)> = Vec::new();
    for &memory_type in types {
        match counts.iter_mut().find(|(seen, _)| *seen == memory_type) {
            Some((_, count)) => *count += 1,
            None => counts.push((memory_type, 1)),
        }
    }
    let parts: Vec<String> = counts
        .into_iter()
        .map(|(memory_type, count)| format!("{} x{count}", type_name(memory_type)))
        .collect();
    parts.join(", ")
}

fn type_name(memory_type: u8) -> String {
    match memory_type {
        0x18 => "DDR3".to_owned(),
        0x1A => "DDR4".to_owned(),
        0x1E => "LPDDR4".to_owned(),
        0x22 => "DDR5".to_owned(),
        0x23 => "LPDDR5".to_owned(),
        other => format!("type 0x{other:02X}"),
    }
}

#[cfg(test)]
mod tests;
