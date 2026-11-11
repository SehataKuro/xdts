//! STS and OpenToonz scenes.
//!
//! Neither format has a specification, and neither application that defined
//! them is something a test can run. So the STS files here are built byte by
//! byte the way each implementation still around builds them — DiTiS, which
//! writes them today, and remaping, which reads the original application's —
//! and the scene is `tests/data/synthetic.tnz`, an invented scene in the
//! shape OpenToonz saves (see NOTICE), with what OpenToonz's own XDTS export
//! would make of it written out below.

use xdts::{sts, tnz, Error, Sheet, TimeTable, Track, HYPHEN, NULL_CELL, TICK_1};

/// An STS file as DiTiS's `generateStsBuffer` writes one: a byte of frame
/// count, a zero after it, cells as [value, 0], names as UTF-8.
fn ditis_sts(names: &[&str], columns: &[&[u8]]) -> Vec<u8> {
    let frames = columns[0].len();
    let mut b = vec![0x11];
    b.extend_from_slice(b"ShiraheiTimeSheet");
    b.push(columns.len() as u8);
    b.push(frames as u8);
    b.extend_from_slice(&[0, 0, 0]);
    for c in columns {
        for &v in *c {
            b.extend_from_slice(&[v, 0]);
        }
    }
    for n in names {
        b.push(n.len() as u8);
        b.extend_from_slice(n.as_bytes());
    }
    b
}

fn table(sheet: &Sheet) -> &TimeTable {
    assert_eq!(sheet.tables.len(), 1);
    &sheet.tables[0]
}

fn values(t: &Track) -> Vec<(usize, &str)> {
    t.frames
        .iter()
        .map(|(f, c)| (*f, c.value.as_str()))
        .collect()
}

#[test]
fn an_sts_from_ditis_reads_as_change_points() {
    let bytes = ditis_sts(&["A", "BG"], &[&[1, 1, 2, 2, 0, 3], &[0, 0, 5, 5, 5, 5]]);
    let s = sts::from_bytes(&bytes).unwrap();
    let t = table(&s);
    assert_eq!(t.duration, 6);
    assert_eq!(t.columns, ["A", "BG"]);
    assert_eq!(
        values(&t.tracks[0]),
        [(0, "1"), (2, "2"), (4, NULL_CELL), (5, "3")]
    );
    assert_eq!(values(&t.tracks[1]), [(2, "5")]);

    // And it is the file this crate writes back, byte for byte.
    assert_eq!(sts::to_bytes(&s).unwrap(), bytes);
    // The top-level reader knows it by its signature.
    assert_eq!(xdts::from_bytes(&bytes).unwrap(), s);
}

/// remaping reads the frame count and every cell as a little-endian u16,
/// and names as Shift_JIS — the original application's reading, and a
/// superset of DiTiS's.
#[test]
fn an_sts_over_255_frames_with_a_shift_jis_name_reads() {
    let frames = 300usize;
    let mut b = vec![0x11];
    b.extend_from_slice(b"ShiraheiTimeSheet");
    b.push(1);
    b.extend_from_slice(&(frames as u16).to_le_bytes());
    b.extend_from_slice(&[0, 0]);
    for f in 0..frames {
        let n: u16 = if f < 150 { 1 } else { 1000 };
        b.extend_from_slice(&n.to_le_bytes());
    }
    // セル in Shift_JIS.
    b.extend_from_slice(&[4, 0x83, 0x5a, 0x83, 0x8b]);

    let s = sts::from_bytes(&b).unwrap();
    let t = table(&s);
    assert_eq!(t.duration, 300);
    assert_eq!(t.columns, ["セル"]);
    assert_eq!(values(&t.tracks[0]), [(0, "1"), (150, "1000")]);
}

#[test]
fn writing_sts_holds_through_hyphens_and_ticks_and_stops_at_x() {
    let s = Sheet {
        tables: vec![TimeTable {
            duration: 6,
            columns: vec!["A".into()],
            tracks: vec![Track {
                frames: vec![
                    (0, "1".into()),
                    (1, TICK_1.into()),
                    (2, HYPHEN.into()),
                    (3, "02".into()),
                    (5, NULL_CELL.into()),
                ],
            }],
            ..TimeTable::default()
        }],
        ..Sheet::default()
    };
    let b = sts::to_bytes(&s).unwrap();
    assert_eq!(b, ditis_sts(&["A"], &[&[1, 1, 1, 2, 2, 0]]));
}

#[test]
fn what_sts_cannot_hold_is_refused() {
    let one = |frames: Vec<(usize, &str)>, duration: usize| Sheet {
        tables: vec![TimeTable {
            duration,
            columns: vec!["A".into()],
            tracks: vec![Track {
                frames: frames.into_iter().map(|(f, v)| (f, v.into())).collect(),
            }],
            ..TimeTable::default()
        }],
        ..Sheet::default()
    };
    let refused = |s: &Sheet| matches!(sts::to_bytes(s), Err(Error::Unsupported(_)));

    assert!(refused(&one(vec![(0, "a0001")], 4)));
    assert!(refused(&one(vec![(0, "1a")], 4)));
    assert!(refused(&one(vec![(0, "0")], 4)));
    assert!(refused(&one(vec![(0, "70000")], 4)));
    assert!(refused(&one(vec![(0, "1")], 70_000)));
    assert!(refused(&Sheet::default()));

    // A letter past the end of the sheet is never shown, so it is no reason
    // to refuse.
    assert!(sts::to_bytes(&one(vec![(0, "1"), (4, "1a")], 4)).is_ok());
}

#[test]
fn a_cut_short_sts_is_an_error_and_a_missing_name_is_not() {
    let full = ditis_sts(&["A"], &[&[1, 2]]);
    assert!(sts::from_bytes(&full[..full.len() - 3]).is_err());
    assert!(sts::from_bytes(b"\x11ShiraheiTime").is_err());

    let s = sts::from_bytes(&full[..full.len() - 2]).unwrap();
    assert_eq!(table(&s).columns, [""]);
}

const SCENE: &str = include_str!("data/synthetic.tnz");

#[test]
fn a_scene_reads_as_opentoonz_exports_it() {
    let s = tnz::from_str(SCENE).unwrap();
    let t = table(&s);

    // The output range is 0–9, so the sheet is ten frames, not the scene's
    // twelve.
    assert_eq!(t.duration, 10);

    // Named after the level each column shows first, not the column; the
    // sound column takes no place, the effect column an empty one, and the
    // empty column at the end none.
    assert_eq!(t.columns, ["A", "BG 01", "", "Sub1"]);

    // Padding gone, increments expanded, a letter kept, a gap closed with ×,
    // and × on the frame after the last cell.
    assert_eq!(
        values(&t.tracks[0]),
        [
            (0, "1"),
            (3, "2"),
            (4, "3"),
            (5, "4"),
            (6, "3a"),
            (7, NULL_CELL),
            (8, "4"),
            (10, NULL_CELL),
        ]
    );
    // A single image with no frame number is drawing 1.
    assert_eq!(values(&t.tracks[1]), [(0, "1"), (10, NULL_CELL)]);
    assert!(t.tracks[2].frames.is_empty());
    // A column starting late opens with ×, and a cell of another level in it
    // is written as empty.
    assert_eq!(
        values(&t.tracks[3]),
        [
            (0, NULL_CELL),
            (2, "1"),
            (4, NULL_CELL),
            (6, "2"),
            (8, NULL_CELL),
        ]
    );

    // The top-level readers know a scene by how it starts.
    assert_eq!(xdts::from_str(SCENE).unwrap(), s);
    assert_eq!(xdts::from_bytes(SCENE.as_bytes()).unwrap(), s);

    // And what comes out is an ordinary sheet: XDTS takes it as it is.
    let x = xdts::xdts::to_string(&s, &Default::default());
    assert_eq!(
        xdts::xdts::from_str(&x).unwrap().tables[0].columns,
        t.columns
    );
}

#[test]
fn a_scene_without_an_output_range_is_as_long_as_the_scene() {
    let s = tnz::from_str(&SCENE.replace("0 9 ", "0 -1 ")).unwrap();
    assert_eq!(table(&s).duration, 12);
}

#[test]
fn what_is_not_a_scene_is_an_error() {
    assert!(tnz::from_str("<xdts></xdts>").is_err());
    assert!(tnz::from_str("<tnz framecount='4'></tnz>").is_err());
    assert!(tnz::from_str("<tnz><xsheet></tnz>").is_err());
    assert!(xdts::from_bytes(b"\xff\xfe not anything").is_err());
}
