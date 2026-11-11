//! The in-tree JSON parser and writer, checked against serde_json.
//!
//! This crate keeps its own JSON code so that nothing that uses it has a
//! dependency, and a hand-written parser is exactly where bugs hide: the one
//! here used to accept `+1`, `\x` and raw control characters, ran out of
//! stack on a few thousand `[`, swallowed the character after a lone
//! surrogate and wrote `0.00001` as `0`. Each of those was found by asking
//! what serde_json would do, so the tests below ask it on every input they
//! have — a list of known traps and some thousands of generated documents —
//! and demand three things:
//!
//! 1. both accept the same documents and refuse the same documents,
//! 2. where both accept, they read the same value, keys in the same order,
//! 3. whatever this crate writes, serde_json reads back as that same value.
//!
//! serde_json is a dev-dependency: it is built for these tests and never for
//! anything that depends on this crate.

use xdts::json::{self, Value};

/// serde_json's value as this crate's, for comparison.
fn ours_of(v: &serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => Value::Number(n.as_f64().expect("a finite number")),
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Array(a) => Value::Array(a.iter().map(ours_of).collect()),
        serde_json::Value::Object(m) => {
            Value::Object(m.iter().map(|(k, v)| (k.clone(), ours_of(v))).collect())
        }
    }
}

/// The three demands, on one input.
fn agree(text: &str) {
    let ours = json::parse(text);
    let theirs = serde_json::from_str::<serde_json::Value>(text);
    match (&ours, &theirs) {
        (Ok(o), Ok(t)) => {
            assert_eq!(*o, ours_of(t), "read differently: {text:?}");
            for written in [o.to_compact(), o.to_pretty(), o.to_pretty_indent(4)] {
                let back = serde_json::from_str::<serde_json::Value>(&written)
                    .unwrap_or_else(|e| panic!("wrote what serde_json refuses ({e}): {written:?}"));
                assert_eq!(ours_of(&back), *o, "wrote a different value for {text:?}");
            }
        }
        (Err(_), Err(_)) => {}
        (Ok(_), Err(e)) => panic!("accepted what serde_json refuses ({e}): {text:?}"),
        (Err(e), Ok(_)) => panic!("refused what serde_json accepts ({e}): {text:?}"),
    }
}

/// The ones that have caught this parser out, and the classic traps.
#[test]
fn the_known_traps() {
    let cases = [
        // Whitespace is four characters, not five.
        "[1]",
        " [1] ",
        "\t\n\r[1]\r\n",
        "\u{c}[1]",
        "[1\u{c}]",
        // Numbers.
        "[0]",
        "[-0]",
        "[1e3]",
        "[1E+3]",
        "[1.5e-3]",
        "[0.1]",
        "[123456789012345]",
        "[9007199254740993]",
        "[1e400]",
        "[-1e400]",
        "[1e-400]",
        "[123.456e78]",
        "[+1]",
        "[01]",
        "[-01]",
        "[.5]",
        "[1.]",
        "[-]",
        "[1e]",
        "[1e+]",
        "[1.e5]",
        "[0x10]",
        "[NaN]",
        "[Infinity]",
        "[-Infinity]",
        "[1_000]",
        "[- 1]",
        // Strings.
        r#"["a"]"#,
        r#"[""]"#,
        r#"["\"\\\/\b\f\n\r\t"]"#,
        r#"["あA"]"#,
        r#"["🎨"]"#,
        r#"["\ud800"]"#,
        r#"["\udc00"]"#,
        r#"["\ud800A"]"#,
        r#"["\ud800\ud800"]"#,
        r#"["\u00"]"#,
        r#"["\u+041"]"#,
        r#"["\uGGGG"]"#,
        r#"["\x41"]"#,
        r#"["\'"]"#,
        r#"["\a"]"#,
        "[\"a\u{1}b\"]",
        "[\"a\tb\"]",
        "[\"a\u{7f}b\"]",
        "[\"\u{2028}\"]",
        r#"["abc"#,
        r#"["\"#,
        // Structure.
        "[]",
        "{}",
        "[[]]",
        "[{}]",
        r#"{"a":1}"#,
        r#"{"a":1,"a":2}"#,
        r#"{"b":1,"a":2,"b":3}"#,
        "[1,]",
        r#"{"a":1,}"#,
        r#"{"a"}"#,
        r#"{"a":}"#,
        "{1:2}",
        "[1 2]",
        "[true false]",
        "[true,false,null]",
        "[tru]",
        "[nul]",
        "[truex]",
        "true",
        "null",
        "1",
        r#""s""#,
        "",
        " ",
        "[",
        "]",
        "{",
        "}",
        r#"{"a":1} x"#,
        "[1]]",
        "[[1]",
        "[,1]",
        "[1,,2]",
    ];
    for text in cases {
        agree(text);
    }
}

/// Nesting is refused at the same depth as serde_json refuses it, and is an
/// error rather than a stack overflow.
#[test]
fn nesting_is_refused_where_serde_json_refuses_it() {
    for depth in [
        1,
        10,
        json::MAX_DEPTH - 1,
        json::MAX_DEPTH,
        json::MAX_DEPTH + 1,
    ] {
        agree(&format!("{}{}", "[".repeat(depth), "]".repeat(depth)));
        agree(&format!(
            "{}1{}",
            "{\"a\":".repeat(depth),
            "}".repeat(depth)
        ));
    }
    // And far past it: an error, not an abort.
    let deep = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
    assert!(json::parse(&deep).is_err());
}

/// A small, fixed random number generator, so a failure is the same failure
/// every time it is run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn pick<'a, T>(&mut self, from: &'a [T]) -> &'a T {
        &from[self.below(from.len())]
    }
}

/// Strings of the characters JSON cares about, almost all of them invalid:
/// this is the edge between accepting and refusing.
#[test]
fn random_text_is_accepted_and_refused_alike() {
    let alphabet: Vec<char> = "[]{},:\"\\/0123456789-+.eEtrufalsnubx \t\n\r\u{c}\u{1}あ🎨"
        .chars()
        .collect();
    let mut rng = Rng(0x5eed_1234_abcd_0001);
    for _ in 0..20_000 {
        let len = rng.below(24);
        let text: String = (0..len).map(|_| *rng.pick(&alphabet)).collect();
        agree(&text);
    }
}

/// A random document, as serde_json would write it.
fn document(rng: &mut Rng, depth: usize) -> serde_json::Value {
    let leaf = depth == 0 || rng.below(3) == 0;
    match if leaf { rng.below(5) } else { 5 + rng.below(2) } {
        0 => serde_json::Value::Null,
        1 => serde_json::Value::Bool(rng.below(2) == 0),
        2 => {
            // Integers, and floats with every sort of exponent.
            let n = match rng.below(4) {
                0 => rng.below(100_000) as f64,
                1 => -(rng.below(1_000) as f64),
                2 => (rng.next() as f64 / u64::MAX as f64) * 1000.0,
                _ => (rng.next() as f64 / u64::MAX as f64) * 10f64.powi(rng.below(40) as i32 - 20),
            };
            serde_json::json!(n)
        }
        3 | 4 => {
            let parts = [
                "a", "あ", "🎨", "\"", "\\", "/", "\n", "\t", "\u{1}", "\u{7f}", "x y",
            ];
            let s: String = (0..rng.below(6)).map(|_| *rng.pick(&parts)).collect();
            serde_json::Value::String(s)
        }
        5 => serde_json::Value::Array(
            (0..rng.below(4))
                .map(|_| document(rng, depth - 1))
                .collect(),
        ),
        _ => {
            let keys = ["a", "b", "timeTables", "フィールド", "", "a b"];
            let mut m = serde_json::Map::new();
            for _ in 0..rng.below(4) {
                m.insert(rng.pick(&keys).to_string(), document(rng, depth - 1));
            }
            serde_json::Value::Object(m)
        }
    }
}

/// Valid documents, both of serde_json's layouts, and each with one
/// character broken: deleted, doubled or replaced.
#[test]
fn random_documents_and_broken_ones_agree() {
    let noise: Vec<char> = "[]{},:\"\\0-.e \u{1}x".chars().collect();
    let mut rng = Rng(0xd0c_0000_7e57_0002);
    for _ in 0..3_000 {
        let doc = document(&mut rng, 5);
        for text in [
            serde_json::to_string(&doc).unwrap(),
            serde_json::to_string_pretty(&doc).unwrap(),
        ] {
            agree(&text);
            let chars: Vec<char> = text.chars().collect();
            for _ in 0..4 {
                let at = rng.below(chars.len());
                let mut broken = chars.clone();
                match rng.below(3) {
                    0 => {
                        broken.remove(at);
                    }
                    1 => broken.insert(at, chars[at]),
                    _ => broken[at] = *rng.pick(&noise),
                }
                agree(&broken.into_iter().collect::<String>());
            }
        }
    }
}

/// The two sheets in tests/data/ that serde_json can read as they stand.
#[test]
fn the_fixtures_read_alike() {
    for name in ["saved_v10.tdts", "synthetic_v7.tdts", "synthetic_v5.xdts"] {
        let text = std::fs::read_to_string(format!("tests/data/{name}")).unwrap();
        let body = text.split_once('\n').unwrap().1;
        agree(body);
    }
}

/// Past a handful of keys an object is looked up by hash rather than by
/// scanning, and a repeated key must still take the later value in the first
/// one's place on both sides of that switch.
#[test]
fn many_keys_and_repeated_ones_agree() {
    let mut rng = Rng(0x000b_1ec7_0000_0003);
    for n in [1, 15, 16, 17, 40, 300] {
        for _ in 0..50 {
            let pairs: Vec<String> = (0..n)
                .map(|_| format!("\"k{}\":{}", rng.below(n.max(2)), rng.below(1000)))
                .collect();
            agree(&format!("{{{}}}", pairs.join(",")));
        }
    }
}

/// A big object is read in time proportional to its size. Looking each key
/// up by scanning the ones before it made this take minutes; it should take
/// well under a second, and the bound is loose so that a slow machine does
/// not fail it.
#[test]
fn a_big_object_is_read_in_linear_time() {
    let body: Vec<String> = (0..100_000).map(|i| format!("\"k{i}\":0")).collect();
    let text = format!("{{{}}}", body.join(","));
    let t = std::time::Instant::now();
    let v = json::parse(&text).unwrap();
    let took = t.elapsed();
    assert!(matches!(v, Value::Object(ref m) if m.len() == 100_000));
    assert!(took.as_secs() < 10, "100,000 keys took {took:?}");
}
