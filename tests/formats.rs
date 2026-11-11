//! Round trips, and conformance against files a real application wrote.
//!
//! Neither format has a specification this crate can check itself against —
//! XDTS has a published schema but TDTS has nothing at all — so the next best
//! thing is the *shape* of a file the software itself produced.
//! `tests/data/real_v7.tdts` and `real_v5.xdts` are synthetic sample sheets
//! from the vdts-editor project (MIT; see NOTICE), and the tests below demand
//! that what this crate writes has the same keys.

use xdts::{
    json, tdts, xdts as x, Cell, Field, Sheet, TimeTable, Track, ValueTrack, WriteOptions,
    FIELD_CELL_ALT, OPTION_KEYFRAME, OPTION_REFERENCEFRAME,
};

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
    // Not just the cells: comparing only those is how the written direction
    // was lost without a test noticing.
    assert_eq!(once.tables[0], twice.tables[0]);
}

/// The written direction above the columns is the sheet's camerawork note,
/// and a real sheet has one. Writing a fixed empty string in its place is
/// silent data loss, and only a test that reads the fixture's own text can
/// say it did not happen.
#[test]
fn the_written_direction_is_not_thrown_away() {
    let s = tdts::from_str(&fixture("real_v7.tdts")).unwrap();
    assert_eq!(s.tables[0].direction, "◇PAN A→B 全尺");

    let again = tdts::to_string(&s, &WriteOptions::default());
    assert!(again.contains("◇PAN A→B 全尺"), "the direction was dropped");
    assert_eq!(
        tdts::from_str(&again).unwrap().tables[0].direction,
        s.tables[0].direction
    );
}

/// A time table keeps the name it has. The application's own sheet1, sheet2,
/// … is what a table with no name of its own gets, not what one with a name
/// is renamed to.
#[test]
fn a_time_table_keeps_its_name() {
    let s = sample();
    assert_eq!(s.tables[0].name, "cut012");
    let back = tdts::from_str(&tdts::to_string(&s, &WriteOptions::default())).unwrap();
    assert_eq!(back.tables[0].name, "cut012");

    // The real sheet's name is the application's own numbering, and it comes
    // back as it was too.
    let real = tdts::from_str(&fixture("real_v7.tdts")).unwrap();
    assert_eq!(real.tables[0].name, "sheet1");

    let mut unnamed = sample();
    unnamed.tables[0].name.clear();
    let text = tdts::to_string(&unnamed, &WriteOptions::default());
    assert!(text.contains("\"name\": \"sheet1\""), "no default name");
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

/// The writer's layout is the applications' layout.
///
/// Parsing keeps key order, so re-writing a file the software itself saved
/// must give that file back byte for byte. This is what catches a writer that
/// is merely valid JSON: 東映 DTS puts every array element on its own line,
/// however short the array, and a sheet written any other way is a different
/// file from the one the application would have written.
#[test]
fn rewriting_a_saved_sheet_gives_the_same_bytes() {
    for (name, pretty) in [("real_v7.tdts", true), ("real_v5.xdts", false)] {
        let text = fixture(name).replace("\r\n", "\n");
        let (head, body) = text.split_once('\n').unwrap();
        let v = json::parse(body).expect("fixture parses");
        let written = if pretty {
            v.to_pretty()
        } else {
            v.to_compact()
        };
        assert_eq!(
            written.trim_end(),
            body.trim_end(),
            "{name} ({head}) is not written back as it was saved"
        );
    }
}

/// The same sheet with the p1 marks on two of its drawings.
fn sample_with_marks() -> Sheet {
    let mut s = sample();
    let frames = &mut s.tables[0].tracks[0].frames;
    frames[0].1 = Cell::marked("1", [OPTION_KEYFRAME]);
    frames[2].1 = Cell::marked("3", [OPTION_REFERENCEFRAME]);
    s
}

/// A sheet with no marks says nothing about any extension, and is the
/// published format exactly. This is the half that matters: using the
/// extension must cost nothing to everyone who does not.
#[test]
fn a_sheet_without_marks_says_nothing_about_p1() {
    let text = x::to_string(&sample(), &WriteOptions::default());
    assert!(!text.contains("subversion"), "an unused extension got out");
    assert!(!text.contains("options"), "an unused extension got out");
    assert_eq!(x::subversion(&sample()), None);
}

/// One marked drawing and the file has to admit which extension put it
/// there, because `options` is not a key the published schema has.
#[test]
fn a_marked_drawing_makes_it_p1() {
    let s = sample_with_marks();
    assert_eq!(x::subversion(&s), Some("p1"));
    let text = x::to_string(&s, &WriteOptions::default());
    assert!(text.contains("\"subversion\": \"p1\""), "no subversion");
    // Every array element is on its own line, as the applications write
    // them, so the key and its value are looked for apart.
    assert!(text.contains("\"options\": ["), "no options");
    assert!(text.contains(OPTION_KEYFRAME), "no keyframe mark");
    // The version is still the published one: p1 and version 10 are
    // unrelated, and a marked sheet with no camera needs nothing from 2022.
    assert!(text.contains("\"version\": 5"), "not version 5");

    let back = x::from_str(&text).unwrap();
    assert_eq!(back.tables[0].tracks, s.tables[0].tracks);
    assert!(back.tables[0].tracks[0].at(0).unwrap().has(OPTION_KEYFRAME));
    assert!(back.tables[0].tracks[0]
        .at(4)
        .unwrap()
        .has(OPTION_REFERENCEFRAME));
    // And the cell is still the cell it was.
    assert_eq!(back.tables[0].tracks[0].value_at(4), Some("3"));
}

/// `options` goes beside `values` in the one `data[]` entry the cell has, in
/// alphabetical order, which is where OpenToonz's writer puts it.
#[test]
fn options_sit_where_a_reader_looks_for_them() {
    let text = x::to_string(&sample_with_marks(), &WriteOptions::default());
    let body = text.split_once('\n').unwrap().1;
    let v = json::parse(body).unwrap();
    let data = v.get("timeTables").unwrap().as_array().unwrap()[0]
        .get("fields")
        .unwrap()
        .as_array()
        .unwrap()[0]
        .get("tracks")
        .unwrap()
        .as_array()
        .unwrap()[0]
        .get("frames")
        .unwrap()
        .as_array()
        .unwrap()[0]
        .get("data")
        .unwrap()
        .as_array()
        .unwrap()[0]
        .clone();
    assert_eq!(data.get("id").unwrap().as_f64(), Some(0.0));
    assert_eq!(
        data.get("options").unwrap().as_array().unwrap()[0].to_text(),
        OPTION_KEYFRAME
    );
    assert_eq!(
        data.get("values").unwrap().as_array().unwrap()[0].to_text(),
        "1"
    );
    // Alphabetical, as the key order of a sheet saved beside one is.
    let keys = json::parse(&data.to_compact()).unwrap();
    let text = keys.to_compact();
    assert!(
        text.find("\"id\"") < text.find("\"options\"")
            && text.find("\"options\"") < text.find("\"values\""),
        "keys are out of order: {text}"
    );
}

/// TDTS is not XDTS, and nothing says the application behind it knows the
/// key. The marks are dropped rather than written into a file that may be
/// refused for having them.
#[test]
fn tdts_does_not_carry_the_marks() {
    let text = tdts::to_string(&sample_with_marks(), &WriteOptions::default());
    assert!(!text.contains("options"), "p1 got into a TDTS file");
    assert!(!text.contains("subversion"), "p1 got into a TDTS file");
    let back = tdts::from_str(&text).unwrap();
    // The cells survive; only the marks on them are gone.
    assert_eq!(back.tables[0].tracks[0].value_at(0), Some("1"));
    assert!(!back.tables[0].tracks[0].has_options());
}

/// A sheet of two time tables is two `timeSheets`, not one `timeSheet` with
/// two revisions in it. An application that shows the latest revision of each
/// sheet would otherwise show one table and hide the other.
#[test]
fn several_tables_are_several_time_sheets() {
    let mut s = sample();
    let mut second = s.tables[0].clone();
    second.name = "cut013".into();
    s.tables.push(second);

    let text = tdts::to_string(&s, &WriteOptions::default());
    let body = text.split_once("\r\n").unwrap().1.to_string();
    let v = json::parse(&body).unwrap();
    let sheets = v.get("timeSheets").unwrap().as_array().unwrap();
    assert_eq!(sheets.len(), 2, "the tables did not get a sheet each");
    for ts in sheets {
        assert_eq!(
            ts.get("timeTables").unwrap().as_array().unwrap().len(),
            1,
            "a sheet was written with a history it does not have"
        );
    }

    let back = tdts::from_str(&text).unwrap();
    assert_eq!(back.tables.len(), 2);
    assert_eq!(back.tables[0].name, "cut012");
    assert_eq!(back.tables[1].name, "cut013");
    for (mine, theirs) in back.tables.iter().zip(&s.tables) {
        assert_eq!(mine.tracks, theirs.tracks);
        assert_eq!(mine.columns, theirs.columns);
        assert_eq!(mine.duration, theirs.duration);
        // The dialogue and camerawork columns come back named S1, S2 and 1, 2:
        // TDTS writes a header for every field whether or not it is used, and
        // those are the application's own defaults, not something this sheet
        // lost or gained.
        assert!(mine.dialogue.is_empty() && mine.camera.is_empty());
    }
}

/// The `timeTables` of one `timeSheet` are revisions of that sheet. The last
/// is the current one, and reading them as separate sheets would turn a
/// sheet's history into extra columns of work that is not there any more.
#[test]
fn the_revisions_of_a_sheet_are_not_sheets() {
    // A file with one sheet saved twice: an older revision, then the current
    // one.
    let old = r#"{"color":0,"direction":"","duration":12,"fields":[{"fieldId":0,
        "tracks":[{"frames":[{"data":[{"fontColorId":0,"id":0,"values":["9"]}],
        "frame":0}],"trackNo":0}]}],"name":"older","timeTableHeaders":[{"fieldId":0,
        "names":["A"]}]}"#;
    let new = old.replace("\"9\"", "\"1\"").replace("older", "current");
    let header = r#"{"cut":"7","episode":"3","scene":""}"#;
    let text = format!(
        "{}\r\n{{\"timeSheets\":[{{\"header\":{header},\"timeTables\":[{old},{new}]}}],\"version\":7}}\r\n",
        tdts::HEADER
    );

    let s = tdts::from_str(&text).unwrap();
    assert_eq!(s.cut, "7");
    assert_eq!(s.episode, "3");
    assert_eq!(s.tables.len(), 1, "a revision was read as a sheet");
    assert_eq!(s.tables[0].name, "current");
    assert_eq!(s.tables[0].tracks[0].value_at(0), Some("1"));
}

/// Several `timeSheets` are several sheets, and all of them are read.
#[test]
fn every_time_sheet_in_a_file_is_read() {
    let two = tdts::to_string(
        &{
            let mut s = sample();
            let mut second = s.tables[0].clone();
            second.name = "cut013".into();
            s.tables.push(second);
            s
        },
        &WriteOptions::default(),
    );
    let back = tdts::from_str(&two).unwrap();
    assert_eq!(back.tables.len(), 2, "a sheet in the file was skipped");
    assert_eq!(back.operator, "nobody");
}

/// A cut number that is not four digits is still that cut number. Writing
/// `112` for `C-112`, or nothing at all for a five digit number, loses what
/// someone wrote on the sheet to satisfy a pattern that a JSON Schema
/// `pattern` does not even enforce — it is a search, so `C-112` matches it.
#[test]
fn a_cut_number_is_not_rewritten() {
    for raw in ["C-112", "12345", "no.3", "カット"] {
        let s = Sheet {
            cut: raw.into(),
            scene: raw.into(),
            tables: sample().tables,
            ..Sheet::default()
        };
        let o = WriteOptions::default();
        assert_eq!(x::from_str(&x::to_string(&s, &o)).unwrap().cut, raw);
        assert_eq!(tdts::from_str(&tdts::to_string(&s, &o)).unwrap().cut, raw);
        // The two writers used to disagree about the same value: TDTS passed
        // it through while XDTS reduced it.
        assert_eq!(
            x::from_str(&x::to_string(&s, &o)).unwrap().scene,
            tdts::from_str(&tdts::to_string(&s, &o)).unwrap().scene
        );
    }
    // And the reduction is still available to whoever wants it.
    assert_eq!(x::header_number("C-112"), "112");
}

/// A sheet's history is kept beside it and written back. The application
/// saves the earlier revisions when it saves the file, and a converter that
/// reads only the current one quietly deletes the rest.
#[test]
fn the_history_of_a_sheet_is_written_back() {
    let mut s = sample();
    let mut older = s.tables[0].clone();
    older.name = "older".into();
    older.tracks[0].frames[0].1 = "9".into();
    s.tables[0].revisions = vec![older.clone()];

    let text = tdts::to_string(&s, &WriteOptions::default());
    let body = text
        .split_once(
            "
",
        )
        .unwrap()
        .1
        .to_string();
    let v = json::parse(&body).unwrap();
    let tables = v.get("timeSheets").unwrap().as_array().unwrap()[0]
        .get("timeTables")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(tables.len(), 2, "the history was not written");
    // Oldest first, current last.
    assert_eq!(tables[0].get("name").unwrap().to_text(), "older");
    assert_eq!(tables[1].get("name").unwrap().to_text(), "cut012");

    let back = tdts::from_str(&text).unwrap();
    assert_eq!(back.tables.len(), 1, "a revision became a sheet");
    assert_eq!(back.tables[0].name, "cut012");
    assert_eq!(back.tables[0].revisions.len(), 1);
    assert_eq!(back.tables[0].revisions[0].name, "older");
    assert_eq!(back.tables[0].revisions[0].tracks[0].value_at(0), Some("9"));
    // A revision has no history of its own.
    assert!(back.tables[0].revisions[0].revisions.is_empty());
}

/// TDTS has two cell columns. Where the other one has the cells, that is
/// where they are read from, and that is where they go back.
#[test]
fn cells_in_the_other_column_are_found() {
    let sheet = format!(
        "{}
{{\"timeSheets\":[{{\"header\":{{\"cut\":\"1\",\"episode\":\"\",\"scene\":\"\"}},\"timeTables\":[{{\"color\":0,\"direction\":\"\",\"duration\":4,\"fields\":[{{\"fieldId\":0,\"tracks\":[]}},{{\"fieldId\":4,\"tracks\":[{{\"frames\":[{{\"data\":[{{\"fontColorId\":1,\"id\":0,\"values\":[\"7\"]}}],\"frame\":0}}],\"trackNo\":0}}]}}],\"name\":\"sheet1\",\"timeTableHeaders\":[{{\"fieldId\":0,\"names\":[]}},{{\"fieldId\":4,\"names\":[\"A\"]}}]}}]}}],\"version\":7}}
",
        tdts::HEADER
    );

    let s = tdts::from_str(&sheet).unwrap();
    let t = &s.tables[0];
    assert_eq!(t.cell_field, FIELD_CELL_ALT, "the other column was ignored");
    assert_eq!(t.tracks[0].value_at(0), Some("7"));
    assert_eq!(
        t.columns,
        ["A"],
        "the column name came from the wrong field"
    );

    // And back into field 4, with field 0 left as the empty column it is.
    let again = tdts::to_string(&s, &WriteOptions::default());
    let back = tdts::from_str(&again).unwrap();
    assert_eq!(back.tables[0].cell_field, FIELD_CELL_ALT);
    assert_eq!(back.tables[0].tracks, t.tracks);
    assert_eq!(back.tables[0].columns, t.columns);

    // A sheet of this crate's own making uses field 0, as the real one does.
    let mine = tdts::to_string(&sample(), &WriteOptions::default());
    assert_eq!(tdts::from_str(&mine).unwrap().tables[0].cell_field, 0);
}

/// The colour a cell is written in, and the sheet's own colour, are the
/// application's and are given back to it.
#[test]
fn the_colours_survive() {
    let mut s = sample();
    s.tables[0].color = 2;
    s.tables[0].tracks[0].frames[0].1 = Cell {
        value: "1".into(),
        options: Vec::new(),
        font_color: 3,
    };

    let back = tdts::from_str(&tdts::to_string(&s, &WriteOptions::default())).unwrap();
    assert_eq!(back.tables[0].color, 2);
    assert_eq!(back.tables[0].tracks[0].at(0).unwrap().font_color, 3);
    // A cell with no colour of its own is the first colour, which is black.
    assert_eq!(back.tables[0].tracks[0].at(2).unwrap().font_color, 0);

    // XDTS has no colours and does not pretend to: the cell survives, the
    // colour does not.
    let x_back = x::from_str(&x::to_string(&s, &WriteOptions::default())).unwrap();
    assert_eq!(x_back.tables[0].tracks[0].value_at(0), Some("1"));
    assert_eq!(x_back.tables[0].tracks[0].at(0).unwrap().font_color, 0);
}
