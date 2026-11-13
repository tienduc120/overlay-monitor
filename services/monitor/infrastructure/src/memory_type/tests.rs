use monitor_domain::MemoryKind;

use super::{memory_kind, memory_text, memory_types};

/// One memory device structure (type 17, formatted length 0x22) followed by its empty string set.
fn device(size: u16, memory_type: u8) -> Vec<u8> {
    let mut s = vec![0_u8; 0x22];
    s[0] = 17;
    s[1] = 0x22;
    s[0x0C..0x0E].copy_from_slice(&size.to_le_bytes());
    s[0x12] = memory_type;
    s.extend([0, 0]);
    s
}

/// A structure of another type with one string, so the walk has to skip its string set.
fn other_structure() -> Vec<u8> {
    let mut s = vec![4, 4, 0, 0];
    s.extend(b"Vendor\0\0");
    s
}

fn table(structures: &[Vec<u8>]) -> Vec<u8> {
    let body: Vec<u8> = structures.concat();
    let mut raw = vec![0, 3, 6, 0];
    raw.extend(u32::try_from(body.len()).unwrap().to_le_bytes());
    raw.extend(body);
    raw
}

#[test]
fn a_ddr5_module_makes_the_machine_ddr5() {
    let raw = table(&[
        other_structure(),
        device(16384, 0x22),
        device(16384, 0x22),
        vec![127, 4, 0, 0, 0, 0],
    ]);

    let types = memory_types(&raw);

    assert_eq!(types, vec![0x22, 0x22]);
    assert_eq!(memory_kind(&types), MemoryKind::Ddr5);
    assert_eq!(memory_text(&types), "DDR5 x2");
}

#[test]
fn empty_slots_are_skipped() {
    let raw = table(&[device(0, 0x22), device(8192, 0x1A)]);

    let types = memory_types(&raw);

    assert_eq!(types, vec![0x1A]);
    assert_eq!(memory_kind(&types), MemoryKind::Other);
}

#[test]
fn ddr4_is_other_and_reads_as_ddr4() {
    let types = memory_types(&table(&[device(8192, 0x1A), device(8192, 0x1A)]));

    assert_eq!(memory_kind(&types), MemoryKind::Other);
    assert_eq!(memory_text(&types), "DDR4 x2");
}

#[test]
fn a_truncated_table_stops_without_a_panic() {
    let mut raw = table(&[device(8192, 0x1A), device(8192, 0x22)]);
    raw.truncate(raw.len() - 20);
    assert_eq!(memory_types(&raw), vec![0x1A], "the cut structure is dropped");

    assert_eq!(memory_types(&raw[..5]), Vec::<u8>::new(), "shorter than the header");
    assert_eq!(memory_types(&[]), Vec::<u8>::new());
    let mut no_terminator = table(&[other_structure()]);
    no_terminator.truncate(no_terminator.len() - 3);
    assert_eq!(memory_types(&no_terminator), Vec::<u8>::new());
}

#[test]
fn no_memory_information_keeps_the_ddr5_default() {
    assert_eq!(memory_kind(&[]), MemoryKind::Ddr5);
    assert_eq!(memory_text(&[0x1A, 0x22, 0x01]), "DDR4 x1, DDR5 x1, type 0x01 x1");
}
