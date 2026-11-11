# xdts

アニメのタイムシートを **XDTS / TDTS** として読み書きする Rust クレートです。**依存クレートはゼロ**で、JSON の読み書きも同梱しています。

- **XDTS**（eXchange Digital Time Sheet、セルシス＋東映アニメーション、2018）— CLIP STUDIO PAINT や OpenToonz が読み書きする交換用形式
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
pub struct Sheet     { cut, scene, episode, operator: String, tables: Vec<TimeTable> }
pub struct TimeTable { name: String, duration: usize, columns: Vec<String>, tracks: Vec<Track> }
pub struct Track     { frames: Vec<(usize, String)> }   // (コマ番号, 値)
```

「1コマ1セル」で考えるアプリケーションは、書き出すときにこの形へ変換します。`Track::at(frame)` でそのコマの値を引けます。

セルの値は動画番号そのものか、次の特殊文字列です。

| 定数 | 意味 |
|---|---|
| `NULL_CELL` | × — ここから空セル |
| `TICK_1` | ○ — 中割 |
| `TICK_2` | ● — リバースシート |
| `HYPHEN` | 直前の指示を継続 |

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

セル欄はどちらも `fieldId` 0 です。台詞欄（3）とカメラワーク欄（5）は書き出しません。

## 正しさの確かめ方

XDTS には[公開スキーマ](https://github.com/shun-iwasawa/XDTS-uext)がありますが、**TDTS には仕様書がありません**。そこで `tests/data/` に**実際のアプリケーションが保存したシート**を置き、書き出した JSON の**キー構成が完全に一致する**ことを両方向で検証しています。

これが無かったために、別のプロジェクトでは `timeSheets` に `version: 5` を書いたファイルを数ヶ月にわたって出し続けていました。

## ライセンス

Apache-2.0。`tests/data/` のサンプルシートは [vdts-editor](https://github.com/lunafuse/vdts-editor)（MIT）の架空の合成データです。NOTICE を参照してください。
