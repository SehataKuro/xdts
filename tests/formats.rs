//! Round trips, and conformance against files a real application wrote.
//!
//! Neither format has a specification this crate can check itself against —
//! XDTS has a published schema but TDTS has nothing at all — so the next best
//! thing is the *shape* of a file the software itself produced.
//! `tests/data/real_v7.tdts` and `real_v5.xdts` are synthetic sample sheets
//! from the vdts-editor project (MIT; see NOTICE), and the tests below demand
//! that what this crate writes has the same keys.

use xdts::{json, tdts, xdts as x, Field, Sheet, TimeTable, Track, ValueTrack, WriteOptions};

fn sample() -> Sheet {
    Sheet {
        cut: "12".into(),
        scene: "3".into(),
        episode: "7".into(),
        operator: "nobody".into(),
        tables: vec![TimeTable {
            name: "cut012".into(),
            duration: 12,
            columns: vec!["A".into(), "B".into()],
            tracks: vec![
                Track {
                    frames: vec![
                        (0, "1".into()),
                        (2, "2".into()),
                        (4, "3".into()),
                        (6, xdts::NULL_CELL.into()),
                        (8, "10".into()),
                        (12, xdts::NULL_CELL.into()),
                    ],
                },
                Track {
                    frames: vec![
                        (0, xdts::NULL_CELL.into()),
                        (2, "1".into()),
                        (3, xdts::TICK_1.into()),
                        (5, xdts::TICK_2.into()),
                        (6, "3".into()),
                        (12, xdts::NULL_CELL.into()),
                    ],
                },
            ],
            ..TimeTable::default()
        }],
    }
}

/// The same sheet with a line and a camera move, the camera move carrying the
/// position that only version 10 has room for.
fn sample_with_camera() -> Sheet {
    let mut s = sample();
    let t = &mut s.tables[0];
    t.dialogue = Field {
        columns: vec!["S1".into()],
        tracks: vec![ValueTrack {
            frames: vec![
                (0, vec!["A".into(), "おはよう".into()]),
                (4, vec![xdts::HYPHEN.into()]),
            ],
        }],
    };
    t.camera = Field {
        columns: vec!["1".into()],
        tracks: vec![ValueTrack {
            frames: vec![
                (
                    0,
                    ["T.U.", "0", "0", "100", "0", "0", "0"]
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                ),
                (
                    11,
                    ["SYMBOL_HYPHEN", "120", "-40", "150", "15", "10", "5"]
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                ),
            ],
        }],
    };
    s
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("tests/data/{name}")).expect("fixture missing")
}

/// Every key path in a document, with array elements collapsed to `[]`.
fn key_paths(v: &json::Value, path: &str, out: &mut Vec<String>) {
    match v {
        json::Value::Object(m) => {
            for (k, child) in m {
                let p = format!("{path}/{k}");
                out.push(p.clone());
                key_paths(child, &p, out);
            }
        }
        json::Value::Array(a) => {
            // One element is enough: every element of a well-formed array has
            // the same shape, and walking all of them only adds duplicates.
            if let Some(first) = a.first() {
                key_paths(first, &format!("{path}[]"), out);
            }
        }
        _ => {}
    }
}

fn paths_of(text: &str) -> Vec<String> {
    let body = text
        .trim_start_matches('\u{feff}')
        .split_once('\n')
        .map(|(_, b)| b)
        .unwrap_or("");
    let mut out = Vec::new();
    key_paths(&json::parse(body).expect("not JSON"), "", &mut out);
    out.sort();
    out.dedup();
    out
}

#[test]
fn xdts_round_trip() {
    let s = sample();
    let text = x::to_string(&s, &WriteOptions::default());
    assert!(text.starts_with("exchangeDigitalTimeSheet Save Data\n"));
    let back = x::from_str(&text).unwrap();
    assert_eq!(back.cut, "12");
    assert_eq!(back.scene, "3");
    assert_eq!(back.tables[0].columns, s.tables[0].columns);
    assert_eq!(back.tables[0].tracks, s.tables[0].tracks);
    // The episode has nowhere to go in XDTS and does not come back; the
    // operator does, through a key the schema does not list.
    assert_eq!(back.episode, "");
    assert_eq!(back.operator, "nobody");
}

#[test]
fn tdts_round_trip() {
    let s = sample();
    let text = tdts::to_string(&s, &WriteOptions::default());
    assert!(text.starts_with("toeiDigitalTimeSheet Save Data\r\n"));
    let back = tdts::from_str(&text).unwrap();
    assert_eq!(back.cut, "12");
    assert_eq!(back.tables[0].tracks, s.tables[0].tracks);
    // TDTS has a place for it, so it survives.
    assert_eq!(back.episode, "7");
    assert_eq!(back.operator, "nobody");
}

/// TDTS is version 7 and wraps its sheets. `timeSheets` with `version: 5` is
/// a shape no reader expects, and is the mistake this crate exists to stop
/// being made twice.
#[test]
fn tdts_is_version_7_and_not_xdts() {
    let text = tdts::to_string(&sample(), &WriteOptions::default());
    assert!(text.contains("\"version\": 7"), "not version 7");
    assert!(text.contains("\"timeSheets\": ["), "sheets are not wrapped");
    for id in 0..=5 {
        assert!(
            text.contains(&format!("\"fieldId\": {id}")),
            "no header for field {id}"
        );
    }
    assert!(text.contains("\"fontColorId\": 0"), "no fontColorId");
    assert!(text.contains("\"workSlip\""), "no work slip");
    assert!(
        !text.replace("\r\n", "").contains('\n'),
        "a bare LF got out"
    );
}

/// A version 5 TDTS has its time tables at the top level, with no wrapper.
#[test]
fn a_version_5_tdts_is_still_read() {
    let body = x::to_value(&sample()).to_pretty();
    let old = format!("{}\n{}\n", tdts::HEADER, body);
    assert_eq!(tdts::from_str(&old).unwrap().cut, "12");
}

/// Our TDTS has exactly the keys a sheet saved by the application has — no
/// more and no less. The furniture (`workSlip`, `books`,
/// `timeTableFontColors`, the headers for columns this crate never fills) is
/// not optional decoration: a file without it is not the format.
#[test]
fn tdts_has_the_same_keys_as_a_real_sheet() {
    let mine = paths_of(&tdts::to_string(&sample(), &WriteOptions::default()));
    let real = paths_of(&fixture("real_v7.tdts"));
    let missing: Vec<&String> = real.iter().filter(|p| !mine.contains(p)).collect();
    let extra: Vec<&String> = mine.iter().filter(|p| !real.contains(p)).collect();
    assert!(
        missing.is_empty(),
        "a real sheet has and we do not: {missing:?}"
    );
    assert!(
        extra.is_empty(),
        "we write and a real sheet does not: {extra:?}"
    );
}

/// Our XDTS has every key a real one has, and the only thing beyond it is
/// `operatorName` — not in the schema, not written by every application, but
/// accepted, and better than dropping a name the caller has in hand.
#[test]
fn xdts_has_the_keys_a_real_sheet_has() {
    let mine = paths_of(&x::to_string(&sample(), &WriteOptions::default()));
    let real = paths_of(&fixture("real_v5.xdts"));
    let missing: Vec<&String> = real.iter().filter(|p| !mine.contains(p)).collect();
    let extra: Vec<&String> = mine
        .iter()
        .filter(|p| !real.contains(p) && *p != "/timeTables[]/operatorName")
        .collect();
    assert!(
        missing.is_empty(),
        "a real sheet has and we do not: {missing:?}"
    );
    assert!(extra.is_empty(), "unexpected keys: {extra:?}");
}

/// With no operator name there is nothing extra at all.
#[test]
fn xdts_without_an_operator_is_exactly_the_schema() {
    let mut s = sample();
    s.operator.clear();
    let mine = paths_of(&x::to_string(&s, &WriteOptions::default()));
    let real = paths_of(&fixture("real_v5.xdts"));
    let extra: Vec<&String> = mine.iter().filter(|p| !real.contains(p)).collect();
    assert!(extra.is_empty(), "unexpected keys: {extra:?}");
}

/// And both fixtures read back, which is the other half of being compatible
/// with them.
#[test]
fn the_real_sheets_can_be_read() {
    let t = tdts::from_str(&fixture("real_v7.tdts")).expect("tdts");
    assert_eq!(t.cut, "001");
    assert_eq!(t.episode, "sample#01");
    assert_eq!(t.tables[0].columns, ["A", "B", "C"]);
    assert_eq!(t.tables[0].duration, 48);
    assert!(!t.tables[0].tracks.is_empty());

    let s = xdts::from_str(&fixture("real_v5.xdts")).expect("by header line");
    assert_eq!(s.cut, "001");
    assert_eq!(s.tables[0].columns, ["A", "B", "C"]);
    // The in-between mark in the sample survives the read.
    assert!(s.tables[0].tracks[0]
        .frames
        .iter()
        .any(|(_, v)| v == xdts::TICK_1));
}

/// Reading a real sheet and writing it again gives back the same cell data.
#[test]
fn a_real_sheet_survives_being_rewritten() {
    let once = tdts::from_str(&fixture("real_v7.tdts")).unwrap();
    let twice = tdts::from_str(&tdts::to_string(&once, &WriteOptions::default())).unwrap();
    assert_eq!(once.tables[0].tracks, twice.tables[0].tracks);
    assert_eq!(once.cut, twice.cut);
    assert_eq!(once.episode, twice.episode);
}

#[test]
fn compact_output_is_the_same_document() {
    let s = sample();
    let compact = x::to_string(&s, &WriteOptions { pretty: false });
    // The header line always ends in a newline; the body is what collapses.
    let body = compact.split_once('\n').unwrap().1;
    assert!(
        !body.trim_end().contains('\n'),
        "the body is not on one line"
    );
    assert_eq!(
        x::from_str(&compact).unwrap(),
        x::from_str(&x::to_string(&s, &WriteOptions::default())).unwrap()
    );
}

/// Version 5 is what a sheet without camera coordinates says, because that is
/// the version every reader has. Nothing in a cells-only sheet needs 10.
#[test]
fn a_sheet_without_a_camera_is_still_version_5() {
    let text = x::to_string(&sample(), &WriteOptions::default());
    assert!(text.contains("\"version\": 5"), "not version 5");
    assert_eq!(x::version(&sample()), 5);
}

/// The 2022 specification widened `version` to [5, 10] and grew the
/// camerawork field's values from the direction alone to the direction plus
/// the camera position. A sheet that uses the position is therefore a version
/// 10 sheet, and saying 5 would be a lie about what is inside.
#[test]
fn camera_coordinates_make_it_version_10() {
    let s = sample_with_camera();
    assert_eq!(x::version(&s), 10);
    let text = x::to_string(&s, &WriteOptions::default());
    assert!(text.contains("\"version\": 10"), "not version 10");

    let back = x::from_str(&text).unwrap();
    assert_eq!(back.tables[0].camera, s.tables[0].camera);
    assert_eq!(back.tables[0].dialogue, s.tables[0].dialogue);
    // All seven values of the move survive, in order.
    assert_eq!(
        back.tables[0].camera.tracks[0].at(11).unwrap(),
        ["SYMBOL_HYPHEN", "120", "-40", "150", "15", "10", "5"]
    );
}

/// A camera move with only a direction is what version 5 always allowed, so
/// it does not force the newer version on a reader that cannot take it.
#[test]
fn a_direction_on_its_own_is_still_version_5() {
    let mut s = sample_with_camera();
    for t in &mut s.tables {
        t.camera.tracks[0].frames = vec![(0, vec!["T.U.".into()]), (11, vec![xdts::HYPHEN.into()])];
    }
    assert_eq!(x::version(&s), 5);
    let back = x::from_str(&x::to_string(&s, &WriteOptions::default())).unwrap();
    assert_eq!(back.tables[0].camera, s.tables[0].camera);
}

/// The dialogue and camerawork fields are written and read, in both formats,
/// so a sheet does not lose them by passing through this crate.
#[test]
fn the_other_two_fields_survive_both_formats() {
    let s = sample_with_camera();
    let o = WriteOptions::default();
    for back in [
        x::from_str(&x::to_string(&s, &o)).unwrap(),
        tdts::from_str(&tdts::to_string(&s, &o)).unwrap(),
    ] {
        assert_eq!(back.tables[0].dialogue, s.tables[0].dialogue);
        assert_eq!(back.tables[0].camera, s.tables[0].camera);
        // And the cells are still the cells.
        assert_eq!(back.tables[0].tracks, s.tables[0].tracks);
    }
}

/// The columns this crate never filled before are a real sheet's columns too,
/// and the fixtures have them.
#[test]
fn a_real_sheet_keeps_its_dialogue_and_camera() {
    for text in [fixture("real_v7.tdts"), fixture("real_v5.xdts")] {
        let once = xdts::from_str(&text).unwrap();
        let t = &once.tables[0];
        assert!(!t.dialogue.is_empty(), "no dialogue was read");
        assert!(!t.camera.is_empty(), "no camerawork was read");
        assert_eq!(t.dialogue.columns, ["S1", "S2"]);

        let rewritten = xdts::from_str(&tdts::to_string(&once, &WriteOptions::default())).unwrap();
        assert_eq!(rewritten.tables[0].dialogue, t.dialogue);
        assert_eq!(rewritten.tables[0].camera, t.camera);
    }
}
