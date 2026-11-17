//! Malformed data files must load with typed defaults and counted skips.
//!
//! `wc3-formats` owns the byte parsers (covered by its
//! `tests/malformed_inputs.rs`); this file covers the typed layer above them:
//! a sheet that does not carry a column the schema asks for must answer with
//! the caller's default **and** increment the `missing_slk_column` counter, so
//! a load report can tell "field absent" from "field present with a bad value".

use wc3_data::table::Table;
use wc3_data::table::TableKind;

fn slk(body: &str) -> Vec<u8> {
    format!("ID;PWXL;N;E\nB;X2;Y2;D0\n{body}E\n").into_bytes()
}

#[test]
fn missing_column_returns_the_typed_default_and_is_counted() {
    // The sheet carries `HP` but not `regenHP`: the schema asks for the latter.
    let bytes = slk("C;X1;Y1;K\"unitID\"\nC;X2;Y1;K\"HP\"\nC;X1;Y2;K\"hfoo\"\nC;X2;Y2;K420\n");
    let table = Table::parse(TableKind::UnitData, &bytes).expect("sheet parses");

    assert!(table.column_present("HP"));
    assert!(!table.column_present("regenHP"));

    let before = wc3_formats::diag::count("missing_slk_column");
    let hp = table.i32("hfoo", "HP").expect("present column reads");
    let regen = table.f32("hfoo", "regenHP").expect("absent column is not an error");
    let after = wc3_formats::diag::count("missing_slk_column");

    assert_eq!(hp, Some(420), "the present column keeps its value");
    assert_eq!(regen, None, "the absent column yields the caller's default");
    assert!(
        after > before,
        "the absent column must be counted (before={before} after={after})"
    );
}

#[test]
fn malformed_cell_value_is_reported_not_panicked() {
    let bytes = slk("C;X1;Y1;K\"unitID\"\nC;X2;Y1;K\"HP\"\nC;X1;Y2;K\"hfoo\"\nC;X2;Y2;K\"not-a-number\"\n");
    let table = Table::parse(TableKind::UnitData, &bytes).expect("sheet parses");
    // Engine semantics take the leading number; with no leading number the cell
    // is `None`, not a panic and not a fabricated 0.
    assert_eq!(table.i32("hfoo", "HP").expect("lossy read"), None);
    // The strict read surfaces the bad text as a typed error.
    assert!(table.i32_strict("hfoo", "HP").is_err());
}

#[test]
fn overlay_on_the_id_column_cannot_hide_the_row_from_itself() {
    // A shadow (overlay) value for a column the SLK does not carry is legal:
    // object data creates columns. Reading it must not panic and must not be
    // counted as a missing column.
    let bytes = slk("C;X1;Y1;K\"unitID\"\nC;X1;Y2;K\"hfoo\"\n");
    let mut table = Table::parse(TableKind::UnitData, &bytes).expect("sheet parses");
    table
        .sheet_mut()
        .set_overlay("hfoo", "regenHP", "1.25");
    assert!(table.column_present("regenHP"));
    let before = wc3_formats::diag::count("missing_slk_column");
    let value = table.f32("hfoo", "regenHP").expect("overlay read");
    let after = wc3_formats::diag::count("missing_slk_column");
    assert_eq!(value, Some(1.25));
    assert_eq!(
        after, before,
        "a column created by overlay data is not a missing column"
    );
}
