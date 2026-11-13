use super::smbios_table;

#[test]
fn this_machine_has_an_smbios_table() {
    let table = smbios_table().unwrap();

    assert!(table.len() > 8, "a header and at least one structure: {}", table.len());
    let declared = u32::from_le_bytes([table[4], table[5], table[6], table[7]]);
    assert_eq!(
        usize::try_from(declared).unwrap(),
        table.len() - 8,
        "the header's length field"
    );
}
