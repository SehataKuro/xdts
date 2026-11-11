# xdts

アニメのタイムシートを **XDTS / TDTS** として読み書きする Rust クレートです。**依存クレートはゼロ**で、JSON の読み書きも同梱しています。

- **XDTS**（eXchange Digital Time Sheet、セルシス＋東映アニメーション、2018）— CLIP STUDIO PAINT や OpenToonz が読み書きする交換用形式。version 5（2018-11-29）と version 10（2022-12-06）の両方に対応
- **TDTS** — 東映アニメーション デジタルタイムシートのネイティブ形式。XDTS に似ていますが**同じではありません**

```toml
[dependencies]
xdts = { git = "https://github.com/SehataKuro/xdts" }
```

## 使い方

```rust
use xdts::{Sheet, TimeTable, Track};

let sheet = Sheet {
    cut: "12".into(),
    tables: vec![TimeTable {
        name: "cut012".into(),
        duration: 24,
        columns: vec!["A".into()],
        tracks: vec![Track {
            frames: vec![
                (0,  "1".into()),
                (12, "2".into()),
                (24, xdts::NULL_CELL.into()),   // × でトラックを閉じる
            ],
        }],
        ..TimeTable::default()
    }],
    ..Sheet::default()
};

std::fs::write("cut012.xdts", xdts::xdts::to_string(&sheet, &Default::default()))?;
std::fs::write("cut012.tdts", xdts::tdts::to_string(&sheet, &Default::default()))?;

// 読み込みは拡張子ではなく先頭行で判別します
let back = xdts::from_str(&std::fs::read_to_string("cut012.tdts")?)?;
```

## モデル

シートは**変化点の列**です。これは両形式がファイルに持っている形そのもので、あるコマの内容は「直前の変化点が言ったこと」です。

```rust
pub struct Sheet      { cut, scene, episode, operator: String, tables: Vec<TimeTable> }
pub struct TimeTable  { name, duration, columns, tracks,        // セル欄（欄0）
                        dialogue: Field, camera: Field }        // 台詞欄（3）、カメラワーク欄（5）
pub struct Track      { frames: Vec<(usize, String)> }           // (コマ番号, 値)
pub struct Field      { columns: Vec<String>, tracks: Vec<ValueTrack> }
pub struct ValueTrack { frames: Vec<(usize, Vec<String>)> }      // (コマ番号, 値の並び)
```

「1コマ1セル」で考えるアプリケーションは、書き出すときにこの形へ変換します。`Track::at(frame)` でそのコマの値を引けます。

セル欄は1コマにつき値が1つなので `Track`、台詞欄とカメラワーク欄は値が並ぶので `ValueTrack` です。並びの順序は仕様書の通りで、台詞欄は〈キャラクター名、台詞〉の1〜2個、カメラワーク欄は〈指示、カメラ位置 X、Y、倍率、回転角、カメラ中心 X、Y〉の1〜7個です。足りない分は埋めません。

セルの値は動画番号そのものか、次の特殊文字列です。

| 定数 | 意味 |
|---|---|
| `NULL_CELL` | × — ここから空セル |
| `TICK_1` | ○ — 中割 |
| `TICK_2` | ● — リバースシート |
| `HYPHEN` | 直前の指示を継続 |

## XDTS の2つの version

公開されている仕様書は2つあり、違いは**1点だけ**です。

| version | 仕様書 | 追加点 |
|---|---|---|
| 5 | *XDTS File Format*（2018-11-29） | — |
| 10 | *XDTS File Format*（2022-12-06） | カメラワーク欄がカメラ位置を持てる |

つまり 2022年版は 2018年版の `version` を `[5, 10]` に広げ、カメラワーク欄の `values` を「指示だけ」から「指示＋数値6つ」にしたものです。

したがって version は選ぶものではなく**結果**で、`xdts::version()` がシートから読み取ります。カメラ位置が入っていれば 10、そうでなければ 5 です。新しい形式を必要としないシートを、古い形式しか読めないリーダーから読めなくしないためです。読み込みは両方を受け付け、知らない version も拒否しません（version 11 のファイルが実在し、中の timeTables は同じ形です）。

欄は3つとも読み書きします。`fieldId` 0（セル）は常に、3（台詞）と 5（カメラワーク）はシートが持っているときに書き出します。

## 2つの形式の違い

**TDTS は「ヘッダ行だけ違う XDTS」ではありません。** ここを間違えると、どのリーダーも受け付けないファイルができます。

| | XDTS | TDTS |
|---|---|---|
| `version` | 5 | **7** |
| シートの入れ物 | 直下の `timeTables` | **`timeSheets` で包む** |
| 改行 | LF | **CRLF** |
| `data[]` | `id`, `values` | **`fontColorId`** が付く |
| `timeTableHeaders` | 書いた欄だけ | **0〜5 の全欄** |
| シートヘッダ | `cut`, `scene` | `episode`, `showHeaderDummy`, `timeTableFontColors`, `workSlip` が付く |
| タイムテーブル | `duration`, `name`, `fields`, `timeTableHeaders` | `books`, `color`, `direction` が付く |

TDTS の version 5 は古い形式で、`timeTables` が直下にあり包みがありません。`from_str` はどちらも読みます。

`episode` は TDTS に置き場所があり XDTS にはないので、**XDTS で往復させると話数は落ちます**。`operator` は XDTS では `operatorName` に書きますが、これは公開スキーマに無いキーです（未知のキーは受け付けられます。空なら書きません）。

セル欄はどちらも `fieldId` 0 です。ただし TDTS の欄 ID については、ある変換実装が欄0を ACTION、欄4を CELL と呼び、両方に値があるとアプリケーションは欄4を優先して表示する、としています。`tests/data/` の実物はセルが欄0に入っていて欄4は空なので、このクレートもそう書きます。欄4に値が入った TDTS を読むとセルは空になります。

公開仕様に FPS の属性はありません。

## 正しさの確かめ方

XDTS には[公開スキーマ](https://github.com/shun-iwasawa/XDTS-uext)（[version 10 の原文 PDF](https://vd.clipstudio.net/clipcontent/paint/app/ToeiAnimation/XDTSFileFormat_ver10_en.pdf)）がありますが、**TDTS には仕様書がありません**。そこで `tests/data/` に**実際のアプリケーションが保存したシート**を置き、書き出した JSON の**キー構成が完全に一致する**ことを両方向で検証しています。

これが無かったために、別のプロジェクトでは `timeSheets` に `version: 5` を書いたファイルを数ヶ月にわたって出し続けていました。

## ライセンス

Apache-2.0。`tests/data/` のサンプルシートは [vdts-editor](https://github.com/lunafuse/vdts-editor)（MIT）の架空の合成データです。NOTICE を参照してください。
