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
pub struct Sheet      { cut, scene, episode, operator: String, tables: Vec<TimeTable>,
                        work_slip: WorkSlip,                     // 作業伝票
                        show_head_dummy: bool, font_colors: Vec<[u8; 3]>,
                        version: u32 }                           // 読んだ TDTS の version
pub struct WorkSlip   { memo: String, processes: Vec<Process>, works: Vec<Work> }
pub struct TimeTable  { name, duration, columns, tracks,        // セル欄
                        cell_field: u32,                        // セル欄の欄ID（0 か 4）
                        dialogue: Field, camera: Field,         // 台詞欄（3）、カメラワーク欄（5）
                        revisions: Vec<TimeTable>,              // 過去のリビジョン（古い順）
                        color: u32, direction: String }         // シートの色、欄の上の指示
pub struct Track      { frames: Vec<(usize, Cell)> }             // (コマ番号, セル)
pub struct Cell       { value: String, options: Vec<String>,     // 値＋p1のマーク
                        font_color: u32 }                        // セルの文字色
pub struct Field      { columns: Vec<String>, tracks: Vec<ValueTrack> }
pub struct ValueTrack { frames: Vec<(usize, Vec<String>)> }      // (コマ番号, 値の並び)
```

「1コマ1セル」で考えるアプリケーションは、書き出すときにこの形へ変換します。`Track::at(frame)` でそのコマのセル、`Track::value_at(frame)` で値だけを引けます。`Cell` は `"1".into()` で作れるので、マークを使わないかぎり値の文字列を書くだけで済みます。

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

## 非公式拡張 p1

公式仕様ではない拡張を1つだけ扱います。[XDTS-uext](https://github.com/shun-iwasawa/XDTS-uext)（スタジオポノックの拡張版1、作者は OpenToonz 開発者の岩澤俊氏）が追加した、**セルに付くマーク**です。

| 定数 | 意味 |
|---|---|
| `OPTION_KEYFRAME` | 原画 — 数字を丸で囲む |
| `OPTION_REFERENCEFRAME` | 中割参考 — 数字を三角で囲む |

```rust
use xdts::{Cell, OPTION_KEYFRAME};

let cell = Cell::marked("1", [OPTION_KEYFRAME]);
```

セルの `data[]` に `options` 配列として入り、ドキュメント直下に `subversion: "p1"` が付きます。**どちらも公開スキーマには無いキーです**。そこで version と同じ扱いにしています——マークが1つも無ければどちらのキーも書かず、出力は公式仕様そのままです。マークが1つでもあれば、ファイルはどの拡張がそれを書いたのかを名乗る必要があります。`xdts::subversion()` がそれを判断します。

これは OpenToonz 本体が読み書きしているキーで（セルマークの保存先です）、推測ではありません。ただし OpenToonz 自身は version 5 しか書かないので、p1 と version 10 は無関係です。

TDTS には書き出しません。TDTS は別の形式で、東映 DTS がこのキーを知っているという根拠が無いためです（読み込み時は落ちます）。

## キーの順序

キーは**ファイルが持っている順**に書きます。これはアルファベット順でも仕様書のプロパティ順でもありません。

| | 順序 |
|---|---|
| 直下 | `timeTables`, `version`, `header` |
| `fields[]` | `tracks`, `fieldId` |
| `tracks[]` | `trackNo`, `frames` |
| `timeTableHeaders[]` | `names`, `fieldId` |

欄やトラックの「中身」が先で、それを識別する番号が後です。**TDTS は逆で、全体がアルファベット順**です（`fieldId`, `tracks` / `frames`, `trackNo`）。形式が違えば書き手も違う、ということです。

JSON にとっては無意味な差ですが、アプリケーションが保存したシートと diff が取れないファイルは信用しにくくなります。この順序については、別のエディタのサンプルと、実際の制作シートから書き出された XDTS の**2つが一致**しています（このクレートは当初アルファベット順に書いていました——それは TDTS の癖で、XDTS の癖ではありませんでした）。

ただし**レイアウトは2つで一致しません**。一方は全体を1行で書き、もう一方は1キーごとに改行してインデントなし・コロンの後に空白なしです。したがって XDTS に TDTS のような「バイト単位で一致」はありません。`WriteOptions::pretty` はこのクレート自身の2つのレイアウトを選ぶだけのものです。

## 2つの形式の違い

**TDTS は「ヘッダ行だけ違う XDTS」ではありません。** ここを間違えると、どのリーダーも受け付けないファイルができます。

| | XDTS | TDTS |
|---|---|---|
| `version` | 5 または 10 | **10** |
| シートの入れ物 | 直下の `timeTables` | **`timeSheets` で包む** |
| 書き出し | LF・インデント2 | LF・**インデント4** |
| `data[]` | `id`, `values` | **`fontColorId`** が付く |
| 包み | — | `timeSheets` の中に **`free`** が並ぶ |
| `timeTableHeaders` | 書いた欄だけ | **0〜5 の全欄** |
| シートヘッダ | `cut`, `scene` | `episode`, `showHeadDummy`, `timeTableFontColors`, `workSlip` が付く |
| タイムテーブル | `duration`, `name`, `fields`, `timeTableHeaders` | `color`, `direction`, `headDummykomas`, `footDummykomas` が付く |

改行は **LF、BOM なし、インデントは4スペース**です。これはアプリケーションが実際に保存したシートを確認した結果で、以前このクレートが CRLF で書いていたのは誤りでした。読み込みは CRLF も BOM も受け付けます。

TDTS の version 5 は古い形式で、`timeTables` が直下にあり包みがありません。`from_str` はどちらも読みます。

### シートとリビジョン

1つの `timeSheet` の中にある `timeTables` は、**そのシートのリビジョン（履歴）**であって別々のシートではありません。アプリケーションは最新のものを表示し、保存時に過去のものを保持します。シートが複数あるときは `timeSheets` が複数になります。

そこで読み込みは各 `timeSheet` の最後の（＝現在の）タイムテーブルをシートとし、**それより前のものを `TimeTable::revisions` に保ちます**。書き出しはタイムテーブル1つにつき `timeSheet` を1つ作り、履歴をその前に戻します。履歴を持つファイルは履歴を保ったまま往復します。

この挙動の根拠は第三者の記述（DiTiS エディタの FORMATS.md）だけで、`tests/data/` の実物は1シート1リビジョンなので裏付けにはなりません。ここを間違えると、シートの履歴が「もう存在しない作業の列」としてシートの横に並ぶことになります。

`episode` と `direction`（`◇PAN A→B 全尺` のような、欄の上に書かれる指示）は TDTS に置き場所があり XDTS にはないので、**XDTS で往復させると落ちます**。TDTS では読んだものをそのまま書き戻します。`name` も同じで、呼び出し側が持っている名前を連番で上書きしません（名前が空のときだけ、アプリケーション自身の `sheet1`, `sheet2`, … を書きます）。`operator` は XDTS では `operatorName` に書きますが、これは公開スキーマに無いキーです（未知のキーは受け付けられます。空なら書きません）。

セル欄は、新しく作るシートではどちらも `fieldId` 0 です。ただし TDTS には**セル欄が2つ**あります。ある変換実装が欄0を ACTION、欄4を CELL と呼び、両方に値があるとアプリケーションは欄4を優先して表示する、としています。そこで**欄4に何か入っていればそちらから読み**、`cell_field` に覚えて**読んだ欄に書き戻します**。`tests/data/` の実物は欄0にセルがあり欄4は空なので、この挙動は手で組んだファイルでテストしています（公式の根拠はありません）。

TDTS が持っていて XDTS が持たないもの——`episode`、`direction`、`color`、セルの文字色（`fontColorId`）、`showHeadDummy`、**作業伝票**（メモ・担当者・枚数）、そしてリビジョン履歴——は、TDTS で読んで TDTS で書くかぎり保たれます。XDTS を経由すると落ちます。

作業伝票は、空であっても**フォーム全体**（3つの `processes`、2つの `works`、各7トラック）を書き出します。モデルが短ければ空で埋めます——フォームが欠けたファイルはこの形式ではないからです。

### version

読み込んだ version を覚えていて、**書き戻すときに同じ番号を書きます**（7以上の場合）。version 7 のファイルを黙って 10 にすると、それを書いたアプリケーションで開けなくなる可能性があるためです。実物の version 7 シートが 10 とキー構成で違うという根拠は無いので、番号だけを尊重します。

version 5 は形が違い（包みが無く `timeTables` が直下）、このクレートは**読めますが書けません**。したがって version 5 のファイルは 10 として書き出されます。これは意図した格上げで、`timeSheets` の包みに `version: 5` と書くのは——このクレートが存在する理由そのものの間違いです。

公開仕様に FPS の属性はありません。

`cut` と `scene` は公開スキーマでは `\d{1,4}` ですが、**書かれた値をそのまま出力します**。JSON Schema の `pattern` はアンカーなしの検索なので `C-112` も `12345` もこのパターンを満たしますし、数字を1つも含まない値の場合は空文字列にしても（`""` も `\d{1,4}` にマッチしないので）適合にはならず、番号だけが失われます。実際 `tests/data/synthetic_v5.xdts` は `"scene": ""` を持っています——アプリケーション自身が、自分のパターンが拒否する値を書いているわけです。仕様より厳しいリーダーに拒否されるのは目に見えて直せますが、カット番号が黙って書き換わるのは直せません。数字だけに落としたい場合は `xdts::header_number()` を呼んでください。

## 正しさの確かめ方

TDTS 側の基準は `tests/data/saved_v10.tdts` です。これはこのクレートが書き出したファイルですが、**キー構成・キー順・インデント・改行を、アプリケーションが実際に保存したシートとバイト単位で突き合わせて確認**しています（そのシート自体と中身は入っていません。NOTICE を参照）。合成データの `synthetic_v7.tdts` は読み込みのテストにのみ使います——これを書き出しの基準にしていたために、このクレートは存在しないキー `showHeaderDummy` を書き続けていました。

XDTS には[公開スキーマ](https://github.com/shun-iwasawa/XDTS-uext)（[version 10 の原文 PDF](https://vd.clipstudio.net/clipcontent/paint/app/ToeiAnimation/XDTSFileFormat_ver10_en.pdf)）がありますが、**TDTS には仕様書がありません**。そこで `tests/data/` に**実際のアプリケーションが保存したシート**を置き、書き出した JSON の**キー構成が完全に一致する**ことを両方向で検証しています。

これが無かったために、別のプロジェクトでは `timeSheets` に `version: 5` を書いたファイルを数ヶ月にわたって出し続けていました。

変化点というモデル自体も実物で確かめています。ある制作シートの XDTS と、同じシートを1コマ1行で書き出した CSV を突き合わせ、XDTS の0始まりのコマ番号が CSV の `N+1` 行に対応し、値が全コマ一致することを確認しました。

## JSON について

JSON の読み書きはこのクレートの中にあり、外部のクレートには依存しません。そのかわり、**テストのときだけ** `serde_json` を検証用の相手として使います（`[dev-dependencies]` なので、このクレートを使う側では一切ビルドされません。`cargo tree -e normal` は依存ゼロのままです）。

`tests/json_oracle.rs` は、既知の罠の一覧と数千件の生成した入力について、両者が

1. 同じ文書を受け入れ、同じ文書を拒否すること
2. 受け入れた文書を、キーの順序まで同じ値として読むこと
3. このクレートが書き出したものを、`serde_json` が同じ値として読み戻すこと

を確かめています。パーサは RFC 8259 に厳密で、例外は先頭の BOM を読み飛ばすことと、入れ子を128段（`serde_json` と同じ）までに制限することの2つだけです。

このテストを入れる前のパーサには、`+1` や `01`、存在しないエスケープ、文字列中の生の制御文字を受け入れる、フォームフィードを空白とみなす、対になっていないサロゲートが後ろの文字を巻き込んで消す、`1e400` を無限大として読む、入れ子1,000段ほどのファイルでスタックを使い果たしてプロセスごと落ちる、小数を4桁に丸めて書く、といったバグがありました。古いパーサに対してこのテストを走らせると、そのほとんどを手書きの罠ではなくランダム生成の入力が自力で見つけます。

## ライセンス

Apache-2.0。`tests/data/` のサンプルシートは [vdts-editor](https://github.com/lunafuse/vdts-editor)（MIT）の架空の合成データです。NOTICE を参照してください。
