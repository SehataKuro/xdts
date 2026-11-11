//! DiTiS's own file.
//!
//! Both fixtures were written by DiTiS's own code — `buildDitisJson` from
//! `js/fileOperations.js`, run under Node — so they are the application's
//! output, not an imitation of it (see NOTICE). `ditis_new.ditis` is a sheet
//! as `createNewSheet` makes it, then edited; `ditis_imported.ditis` is
//! `tests/data/csp_v5.xdts` brought in by `importExchangeTimesheet`, which is
//! the sheet DiTiS keeps its source document in.

use xdts::{ditis, Format, Sheet, TimeTable, Track, WriteOptions, NULL_CELL, TICK_1};

const NEW: &str = include_str!("data/ditis_new.ditis");
const IMPORTED: &str = include_str!("data/ditis_imported.ditis");

fn values(t: &Track) -> Vec<(usize, &str)> {
    t.frames
        .iter()
        .map(|(f, c)| (*f, c.value.as_str()))
        .collect()
}

#[test]
fn a_new_sheet_reads_as_its_changes_and_writes_back_byte_for_byte() {
    let s = ditis::from_str(NEW).unwrap();
    assert_eq!(s.read_from, Some(Format::Ditis));
    let t = &s.tables[0];
    assert_eq!(t.name, "Sheet1");
    assert_eq!(t.duration, 12);
    assert_eq!(t.columns.len(), 26);
    assert_eq!(&t.columns[..2], ["A", "背景"]);

    // The same changes DiTiS's own XDTS export works out of this file.
    assert_eq!(
        values(&t.tracks[0]),
        [
            (0, "1"),
            (3, "2"),
            (5, NULL_CELL),
            (6, TICK_1),
            (8, "3"),
            (10, NULL_CELL),
        ]
    );
    assert_eq!(values(&t.tracks[1]), [(0, "5")]);
    assert!(t.tracks[2].frames.is_empty());

    // fps, the page size, the inserted and disabled rows: kept.
    let keys: Vec<_> = t.extra.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(
        keys,
        ["fps", "framePageSize", "insertedFrames", "disabledFrames"]
    );

    assert_eq!(ditis::to_string(&s, &WriteOptions::default()), NEW);
    assert_eq!(xdts::from_str(NEW).unwrap(), s);
}

#[test]
fn an_imported_sheet_keeps_its_source_document() {
    let s = ditis::from_str(IMPORTED).unwrap();
    let t = &s.tables[0];
    assert_eq!(t.duration, 48);
    assert_eq!(
        values(&t.tracks[0]),
        [(0, "1"), (6, "2"), (12, "3"), (24, "4")]
    );
    assert!(t.extra.iter().any(|(k, _)| k == "timesheetSource"));

    // Written again and read again, it is the same sheet: DiTiS's source
    // document and all. What differs on the page is where `frames` sits,
    // and a column DiTiS filled with × to the end coming back empty.
    let again = ditis::from_str(&ditis::to_string(&s, &WriteOptions::default())).unwrap();
    let column = |s: &Sheet, c: usize| {
        let t = &s.tables[0];
        (0..t.duration)
            .map(|f| t.tracks[c].value_at(f).filter(|v| *v != NULL_CELL))
            .map(|v| v.map(str::to_string))
            .collect::<Vec<_>>()
    };
    for c in 0..t.columns.len() {
        assert_eq!(column(&again, c), column(&s, c), "column {c}");
    }
    assert_eq!(again.tables[0].extra, t.extra);
    assert_eq!(again.extra, s.extra);
}

#[test]
fn a_sheet_from_elsewhere_is_written_as_a_new_ditis_sheet() {
    let s = Sheet {
        tables: vec![TimeTable {
            duration: 4,
            columns: vec!["A".into(), "B".into()],
            tracks: vec![Track {
                frames: vec![(0, "1".into()), (2, NULL_CELL.into())],
            }],
            ..TimeTable::default()
        }],
        ..Sheet::default()
    };
    let text = ditis::to_string(&s, &WriteOptions { pretty: false });
    assert_eq!(
        text,
        r#"{"version":"1.0","sheets":[{"name":"Sheet1","fps":24,"layers":["A","B"],"frames":4,"framePageSize":4,"data":[["1","1"]],"insertedFrames":[],"disabledFrames":[]}]}"#
    );

    // Keys another format kept do not leak into a DiTiS file.
    let x = xdts::xdts::from_str(include_str!("data/csp_v5.xdts")).unwrap();
    let text = ditis::to_string(&x, &WriteOptions::default());
    assert!(text.contains("\"fps\": 24"));
    assert_eq!(ditis::from_str(&text).unwrap().tables.len(), x.tables.len());
}

#[test]
fn an_older_file_with_layers_by_id_reads() {
    let s = ditis::from_str(
        r#"{"version":"1.0","sheets":[{"name":"old","frames":3,
            "layers":[{"id":"L1","name":"A"},{"id":"L2","name":"B"}],
            "data":{"1":{"L1":"1","L2":"2"},"2":{"L1":"1"},"3":{"L1":"3","L2":"2"}}}]}"#,
    )
    .unwrap();
    let t = &s.tables[0];
    assert_eq!(t.columns, ["A", "B"]);
    assert_eq!(values(&t.tracks[0]), [(0, "1"), (2, "3")]);
    assert_eq!(values(&t.tracks[1]), [(0, "2"), (1, NULL_CELL), (2, "2")]);
}

#[test]
fn what_is_not_a_ditis_file_is_an_error() {
    assert!(ditis::from_str("[]").is_err());
    assert!(ditis::from_str(r#"{"sheets":[]}"#).is_err());
    assert!(ditis::from_str(r#"{"sheets":[{"name":"x"}]}"#).is_err());
}
