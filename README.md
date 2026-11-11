# xdts

アニメのタイムシートを **XDTS / TDTS / STS** として読み書きし、**OpenToonz のシーン（TNZ）** からタイムシートを読み出す Rust クレートです。**依存クレートはゼロ**で、JSON の読み書きも同梱しています。

- **XDTS**（eXchange Digital Time Sheet、セルシス＋東映アニメーション、2018）— CLIP STUDIO PAINT や OpenToonz が読み書きする交換用形式。version 5（2018-11-29）と version 10（2022-12-06）の両方に対応
- **TDTS** — 東映アニメーション デジタルタイムシートのネイティブ形式。XDTS に似ていますが**同じではありません**
- **STS** — シラヘイタイムシートのバイナリ形式。今は DiTiS が読み書きしています（[STS](#sts)）
- **DiTiS** — DiTiS の独自形式（`.ditis` / `.json`）。セル欄を読み書きし、このクレートがモデルに持たないもの（FPS、挿入・無効行、取り込み元の XDTS / TDTS など）も**そのまま保って書き戻します**（[DiTiS](#ditis)）
- **TNZ** — OpenToonz / Tahoma2D のシーンファイル。**読み込みのみ**で、OpenToonz 自身の XDTS 書き出しと同じ解釈でセル欄を取り出します（[OpenToonz のシーン](#opentoonz-のシーンtnz)）

形式が分からないファイルは `xdts::from_bytes` に渡せば、中身の先頭で判別して読みます（拡張子は見ません）。

```toml
[dependencies]
xdts = "0.1"
```

API ドキュメントは [docs.rs/xdts](https://docs.rs/xdts) にあります。

## 使い方

```rust,no_run
use xdts::{Sheet, TimeTable, Track};

fn main() -> Result<(), Box<dyn std::error::Error>> {
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
    assert_eq!(back.cut, "12");
    Ok(())
}
```

## モデル

シートは**変化点の列**です。これは両形式がファイルに持っている形そのもので、あるコマの内容は「直前の変化点が言ったこと」です。

```rust,ignore
pub struct Sheet       { cut, scene, episode, operator: String, tables: Vec<TimeTable>,
                         work_slip: WorkSlip,                     // 作業伝票
                         show_head_dummy: bool, font_colors: Vec<[u8; 3]>,
                         version: u32,                            // 読んだ TDTS の version
                         read_from: Option<Format>,               // 読んだ形式
                         extra: Extra, header_extra: Extra }      // 知らないキー
pub struct WorkSlip    { memo: String, processes: Vec<Process>, works: Vec<Work>, extra: Extra }
pub struct TimeTable   { name, duration, columns, tracks,        // セル欄
                         cell_field: u32,                        // セル欄の欄ID（0 か 4）
                         dialogue: Field, camera: Field,         // 台詞欄（3）、カメラワーク欄（5）
                         revisions: Vec<TimeTable>,              // 過去のリビジョン（古い順）
                         color: u32, direction: String,          // シートの色、欄の上の指示
                         extra: Extra, sheet_extra: Extra }      // 知らないキー
pub struct Track       { frames: Vec<(usize, Cell)> }             // (コマ番号, セル)
pub struct Cell        { value: String, options: Vec<String>,     // 値＋p1のマーク
                         font_color: u32 }                        // セルの文字色
pub struct Field       { columns: Vec<String>, tracks: Vec<ValueTrack> }
pub struct ValueTrack  { frames: Vec<(usize, Instruction)> }      // (コマ番号, 指示)
pub struct Instruction { values: Vec<String>, font_color: u32 }   // 値の並び＋文字色
pub type   Extra       = Vec<(String, json::Value)>;
```

「1コマ1セル」で考えるアプリケーションは、書き出すときにこの形へ変換します。`Track::at(frame)` でそのコマのセル、`Track::value_at(frame)` で値だけを引けます。`Cell` は `"1".into()` で作れるので、マークを使わないかぎり値の文字列を書くだけで済みます。

セル欄は1コマにつき値が1つなので `Track`、台詞欄とカメラワーク欄は値が並ぶので `ValueTrack` です（`["A", "おはよう"].into()` で1つの指示になります）。並びの順序は仕様書の通りで、台詞欄は〈キャラクター名、台詞〉の1〜2個、カメラワーク欄は〈指示、カメラ位置 X、Y、倍率、回転角、カメラ中心 X、Y〉の1〜7個です。足りない分は埋めません。

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

| | XDTS（クリスタの順） | TDTS（アルファベット順） |
|---|---|---|
| 直下 | `version`, `header`, `timeTables` | `timeSheets`, `version` |
| `timeTables[]` | `name`, `duration`, `timeTableHeaders`, `fields` | `color`, `direction`, `duration`, `fields`, … |
| `fields[]` | `fieldId`, `tracks` | `fieldId`, `tracks` |
| `tracks[]` | `trackNo`, `frames` | `frames`, `trackNo` |
| `frames[]` | `frame`, `data` | `data`, `frame` |
| `timeTableHeaders[]` | `fieldId`, `names` | `fieldId`, `names` |

XDTS は**番号が先で、それが数える中身が後**です（`trackNo` → `frames`、`frame` → `data`）。TDTS は全体がアルファベット順なので、その2つは逆になります。`fieldId` はどちらも先頭です——クリスタの順がそうで、かつアルファベット順でも `fieldId` が先に来るからで、ここだけは2つの形式が一致します。

JSON にとっては無意味な差ですが、アプリケーションが保存したシートと diff が取れないファイルは信用しにくくなります。根拠はそれぞれ**実際のアプリケーションが書いたファイル1つ**です——TDTS は東映 DTS が保存したシート、XDTS は CLIP STUDIO PAINT EX が書き出したシートで、どちらも**バイト単位で一致**することを `tests/data/saved_v10.tdts` と `tests/data/csp_v5.xdts` に対して検証しています。

`tests/data/synthetic_v5.xdts` は**この根拠ではありません**。別のエディタが書いた架空のサンプルで、キーの並びはクリスタのものと違います。これを XDTS の並びの根拠として読んだために、このクレートは一度 XDTS をアルファベット順で書いていました——それは TDTS の癖で、XDTS の癖ではありませんでした。`synthetic_v7.tdts` が `showHeaderDummy` という保存シートに無いキーを持っているのと同じ話で、エディタの出力はその形式の根拠にはなりません。

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
| タイムテーブル | `name`, `duration`, `timeTableHeaders`, `fields` | `color`, `direction`, `headDummykomas`, `footDummykomas` が付く |

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

## 知らないキー

このクレートが知らないキーは、捨てずに `Extra` に**読んだ順のまま**保ち、書き出すときに戻します。対象は、新しいキーが実際に現れてきた5つの階層——文書の最上位、TDTS の `timeSheets[]` の各要素、ヘッダ、作業伝票、タイムテーブル——です。

これまでに直したものの大半——`direction`、シート名、色、ダミーコマ、作業伝票——は、どれも「知らないキーを黙って捨てる」という同じ間違いでした。見つかるたびに1つずつモデルに場所を作ってきましたが、実物のシートとのバイト一致は「そのシートにあったキーを全部知っている」ことしか保証しません。まだ見つかっていないキーのための場所が `Extra` です。

- **戻す位置**：TDTS ではアルファベット順の位置（保存されたシートはどの階層でもアルファベット順です）。XDTS は順序がアルファベット順ではないので、そのオブジェクトの末尾です。
- **何を「知らない」とみなすか**：その形式の書き手がモデルから書くキー以外すべてです。したがって、このクレートが読みはするがその形式では書かないキー（XDTS ファイル中の `direction` など）も保ちます。
- **形式をまたがない**：保ったキーは、読んだ形式にだけ書き戻します（`Sheet::read_from`）。TDTS の `books` が XDTS に紛れ込むことはありません。
- `free` もこの仕組みで扱います。中身があれば保ち、無ければアプリケーションと同じく空の `free` を書きます。

`fields[]` から下（トラック、フレーム、指示）は仕様で形が決まっていて、新しいキーが現れたことがないので対象外です。

保ったキーは JSON の値で、JSON には浮動小数点があるため、`Sheet` と `TimeTable` は `PartialEq` であって `Eq` ではありません。

## STS

シラヘイタイムシートの `.sts` です。元のアプリケーションは配布サイトが閉鎖されていて、仕様書もありません。そこで、いま残っている2つの実装——元のアプリが保存したファイルを読む **remaping**（`lib_STS.js`）と、いま `.sts` を書いている **DiTiS**——が一致するところを形式とし、食い違うところは両方を受け付ける読み方にしています。

```text
0     1     0x11（続く名前の長さ）
1     17    "ShiraheiTimeSheet"
18    1     列数
19    2     コマ数（リトルエンディアン）
21    2     予約（0）
23    2×列×コマ  セル。列ごとに、1コマ1つの u16（動画番号、0 は空）
…     列名。長さ1バイト＋その長さのバイト列、を列の数だけ
```

食い違いは3つで、どれも DiTiS の読み方が remaping の読み方の特殊な場合になっています。

- **コマ数**：remaping は 19 から u16、DiTiS は 19 の1バイト（20 に 0 を書く）。DiTiS は 255 コマを超えるシートを扱えません
- **セル**：remaping は u16、DiTiS は下位バイト（上位に「type」0 を書く）
- **列名**：元のアプリは日本語 Windows のもので remaping は Shift_JIS、DiTiS は UTF-8。**UTF-8 として正しければ UTF-8**（ASCII の名前はどちらとしても同じ）、そうでなければ Shift_JIS として読みます。Shift_JIS は日本語 Windows のもの（CP932、NEC・IBM 拡張を含む）で、**漢字も読めます**。変換表は Python の `cp932` コーデックから `tools/gen-cp932.py` で生成してクレートに同梱しているので、依存はゼロのままです。書き出しは UTF-8 です

STS が持てるのは**1シート分の動画番号だけ**です（1〜65535 または空）。カット番号・FPS・台詞・カメラ・中割記号・文字付きの番号はありません。そのため `sts::to_bytes` は、書けないものを別の値に置き換えずに `Error::Unsupported` で断ります——タイムテーブルが1つでないシート、`a0001`・`1a`・`0` のような番号、256 列以上、255 バイトを超える列名、65535 コマを超えるシートです。`SYMBOL_HYPHEN` と中割記号 ○● は OpenToonz の読み方と同じく直前の番号を続け、記号そのものは残りません。256 コマ以上のシートはここと remaping では正しいファイルですが、DiTiS では開けません。

## DiTiS

DiTiS の独自形式は、ヘッダ行のない JSON です（DiTiS の `buildDitisJson` / `loadSheetData`）。

```json
{ "version": "1.0",
  "sheets": [{ "name": "Sheet1", "fps": 24, "layers": ["A", "B"], "frames": 144,
               "framePageSize": 144, "data": [["1", "1", "2"]],
               "insertedFrames": [], "disabledFrames": [] }] }
```

交換形式と違い、**列はコマごとの値を全部持ちます**（`"1", "1", "2"` は1が2コマ、次に2）。列は最後の値で、列の並びは最後の空でない列で切れます。値は DiTiS 独自のもので、絵の番号、`×`（明示した空）、`○` `●`（中割）、`""`（何もない）、古い版が「前のコマと同じ」の意味で書いていた `-` です。`layers` が `{id, name}` で `data` がコマ・列 ID で引く形の古いファイルも読みます。

- **読み込み**は DiTiS 自身の XDTS 書き出し（`buildTimesheetTable`）と同じく変化点に直します。`×` と `""` はどちらも `SYMBOL_NULL_CELL`、`○` `●` は `SYMBOL_TICK_1` / `SYMBOL_TICK_2`、`-` は `SYMBOL_HYPHEN` です
- **書き出し**は DiTiS の取り込み（`importExchangeTimesheet`）と同じく、各値を次の変化まで埋めます（空は `×`、`SYMBOL_HYPHEN` は続けている番号）。ただし列の**末尾の空は書きません**（DiTiS 自身がそう保存します）
- **バイト単位の往復**：DiTiS から読んだシートは、モデルが持つキー（`name`・`frames`・`layers`・`data`）も含めた**全キーをファイルの順で** `TimeTable::extra` に保ち、その順で書きます（モデルが持つキーの値はモデルから書きます）。`data` はセルが変わっていない間は**読んだまま**書き戻すので、空のコマが `×` だったか `""` だったか、`-` だったかも残ります。セルを変えると `data` はモデルから書き直します
- **モデルに無いもの**——FPS、ページの長さ、挿入・無効行、メモ、そして `timesheetSource`（取り込み元の XDTS / TDTS 文書。DiTiS が編集できない台詞・カメラ・色はここに残っています）——は `TimeTable::extra` と `Sheet::extra` に読んだまま保ち、**DiTiS の形式にだけ**書き戻します。DiTiS 以外から来たシートは、新規の DiTiS シートと同じもの（24fps、1ページ、挿入・無効行なし）を付けて書きます
- DiTiS が持つのはセル欄だけなので、シートの台詞欄・カメラ欄と p1 の印は書きません

**DiTiS のコードそのものを Node で動かして**確かめています。DiTiS が書いた新規シートはバイト単位で同じに戻り、XDTS に書き出した変化点も DiTiS 自身の書き出しと一致します。DiTiS が XDTS から取り込んだシートも、バイト単位で同じに戻ります。`tests/data/ditis_*.ditis` はこのとき DiTiS のコードが書いたファイルです。

## OpenToonz のシーン（TNZ）

`.tnz` は OpenToonz / Tahoma2D のシーン全体（カメラ、出力設定、レベルファイルの場所、ペグバー、エフェクト）ですが、ここで読むのは交換用タイムシートに置き場所のある部分——**各列が各コマにどの絵を出しているか**——だけです。読み方は **OpenToonz 自身の XDTS 書き出し**（`toonz/sources/toonz/xdtsio.cpp`）に合わせていて、同じシーンから OpenToonz が書き出す XDTS と同じシートになります。

- 列はトップの Xsheet の順。サウンド列はセルを持たないので詰め、それ以外の列は（空でも）1列を占めます
- 列名は**列の名前ではなく、その列が最初に出しているレベルの名前**です。同じ列にある別のレベルのセルは空として書きます。絵のレベルもサブ Xsheet も出していない列（パレット、エフェクト、台詞）は空の列です
- 番号はフレーム番号から桁埋めを除き、文字があれば付けます（`0001` → `1`、`0012a` → `12a`）
- **ここだけは意図して書き出しと違います**：番号のない1枚絵（ファイル中の `-`、`.tif` 1枚など）は `1` です。OpenToonz は「番号なし」の内部値 −2 をそのまま `"-2"` と書き出し（`xdtsio.cpp` の `fid2Str`）、他のアプリケーションはそれを −2 番の絵として読んでしまうためです
- 途中から始まる列は 0 コマ目に ×、どの列も最後のセルの次のコマに × が付きます（書き出しと同じ）
- 長さは出力範囲が設定されていればその範囲、なければシーンの長さです

- 中割（○●）と原画・中割参考の印は、書き出しと同じく**セルマーク**から読みます。どの印が何を表すかはファイルになく書き出しダイアログで選ぶものなので、`tnz::CellMarks` で指定します。既定値はダイアログの既定値（既定のマーク一覧で青＝○、紫＝●、赤＝原画、緑＝中割参考）で、`CellMarks::NONE` なら印を読みません。○● は前の絵が続いているコマで、原画・中割参考は絵が始まるコマで読み、原画・中割参考があると XDTS 書き出しが p1 を名乗ります

書き出しが持たないものとして、**台詞**も読みます。台詞列（サウンドテキスト列）は台詞欄の列になり、列名（なければレベル名）を話者として、台詞の始まるコマに〈話者、台詞〉、続くコマに `SYMBOL_HYPHEN` を置きます。この並べ方は手元で唯一台詞を持つシート `tests/data/synthetic_v5.xdts`（別のエディタの架空のサンプル）に倣ったもので、アプリケーションが期待する形だという根拠ではありません。台詞列はセル欄でも書き出しと同じく空の列を1つ占めます。

読まないもの：サブ Xsheet の中身、FPS。タイムテーブル名は空です——OpenToonz はシーンのファイル名を使いますが、それはファイルの中にありません。

ファイルは OpenToonz の `TOStream` が書いたもので、XML に見えますが XML ではありません（`&` や `<` がエスケープされず、テキストはバックスラッシュでエスケープした引用符付き文字列を含むトークン列で、2回目に出てくるオブジェクトは `<level id='9'/>` のような参照です）。そのためこの形式専用の読み取りを持っています。

**書き出しには対応しません。** 開けるシーンには、そこに書かれたレベルファイルが必要で、交換用タイムシートはそれを持っていないからです。OpenToonz は XDTS を直接読めます。

`tests/data/synthetic.tnz` は OpenToonz が保存する形で書いた架空のシーンです。OpenToonz の公式サンプルシーン（[opentoonz_sample](https://github.com/opentoonz/opentoonz_sample)）3本でも読み出しを確認していますが、ライセンスが非営利限定（CC BY-NC 4.0）のためリポジトリには入れていません。

## 正しさの確かめ方

形式ごとに何を根拠にし、何を確かめ、何を確かめていないかは [FORMATS.md](FORMATS.md) にまとめています。DiTiS との突き合わせは `node tools/check-ditis.mjs /path/to/ditis` で再現できます。

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

## CI

push のたびに GitHub Actions で次を確かめます（`.github/workflows/ci.yml`）。

- `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo doc`（警告をエラーとして）
- `cargo test` を **Linux と Windows** で。改行はこのクレートが正しくも間違ってもきた部分で、fixture がバイト単位のままテストに届くか（`.gitattributes` が効いているか）は Windows のチェックアウトでしか分かりません
- `Cargo.toml` の `rust-version`（1.85）でライブラリがビルドできること

外部の Action は `actions/checkout` だけで、Rust はランナーに入っている rustup で用意します。

## ライセンス

Apache-2.0。`tests/data/` のファイルの出どころ（[vdts-editor](https://github.com/lunafuse/vdts-editor) の合成データ、DiTiS のコードが書いたファイル、手で書いたシーンなど）とそれぞれのライセンスは NOTICE にあります。
