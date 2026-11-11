# 形式ごとの対応と検証

このクレートが各形式をどこまで扱い、それを何で確かめたかの記録です。形式の細部は [README](README.md) にあります。ここでは「何を根拠にしたか」「何を確かめたか」「何を確かめていないか」を分けて書きます。

根拠の強さは次の3段階で書き分けます。

- **実物**：その形式を定めたアプリケーション、またはそれを読み書きするアプリケーションが実際に書いた・読んだファイルと突き合わせたもの
- **第三者実装**：別の実装のソースコードや、その実装を実際に動かした結果に合わせたもの。公式の根拠ではない
- **合成**：仕様や構造から手で作ったデータ。形の確認にはなるが、アプリケーションとの互換の証拠にはならない

## 確認資料（2026-11-12）

### XDTS

- 公式仕様 [XDTS File Format 2018-11-29](https://vd.clipstudio.net/clipcontent/paint/app/ToeiAnimation/XDTSFileFormat_ja.pdf)（version 5）と [2022-12-06](https://vd.clipstudio.net/clipcontent/paint/app/ToeiAnimation/XDTSFileFormat_ver10_ja.pdf)（version 5 / 10。英語版は同じディレクトリの `XDTSFileFormat_ver10_en.pdf`）
- 公開 JSON Schema と非公式拡張 p1：[XDTS-uext](https://github.com/shun-iwasawa/XDTS-uext)
- **実物**：CLIP STUDIO PAINT EX が書き出した制作シート1枚（version 5）。制作物なのでリポジトリには入れず、同じ形で中身を架空にした `tests/data/csp_v5.xdts` を基準にしている
- OpenToonz の XDTS 入出力：`toonz/sources/toonz/xdtsio.cpp`（下の TNZ と同じコミット）

### TDTS

- 公開仕様・公開スキーマは**見つかっていない**
- **実物**：東映アニメーション デジタルタイムシートが保存したシート1枚（version 10）。リポジトリには入れず、同じ形で中身を架空にした `tests/data/saved_v10.tdts` を基準にしている
- **第三者実装**：DiTiS の [FORMATS.md](https://github.com/MisakiAkatsuki/ditis/blob/main/FORMATS.md)（2026-10-12）が参照する [remaping の lib_TDTS.js](https://github.com/kiyo-nekomataya/remaping/blob/master/www/nas/lib/lib_TDTS.js)。リビジョン履歴と欄4の扱いはこれによる。公式仕様ではない

### STS（シラヘイタイムシート）

- 元のアプリケーション（ShiratakeSoftware）は配布サイトが閉鎖されている（[Internet Archive](https://web.archive.org/web/20150920054052/http://striping2nd.or.tv/sts/)）。仕様書は無い
- **第三者実装**：[remaping](https://github.com/kiyo-nekomataya/remaping) の `www/nas/lib/lib_STS.js`（コミット 3633796、2025-08-22）。元のアプリが保存したファイルを読むための実装
- **第三者実装（実行）**：[DiTiS](https://github.com/MisakiAkatsuki/ditis) の `js/parsers.js` の `generateStsBuffer` / `parseStsFile`（コミット 0405796、v2026.10.13.2）。いま `.sts` を書いている実装で、`tools/check-ditis.mjs` が実際に動かす

### DiTiS 独自形式（.ditis / .json）

- 公開仕様は無い。形式を定めているのは DiTiS 自身のコード
- **実物（実行）**：DiTiS の `js/fileOperations.js` の `buildDitisJson` / `normalizeSheetForSave` / `loadSheetData` と、`js/parsers.js` の `buildTimesheetTable` / `generateXdtsData` / `importExchangeTimesheet`（コミット 0405796）。`tools/check-ditis.mjs` がこれらを Node で動かし、`tests/data/ditis_*.ditis` はこのとき DiTiS のコードが書いたファイル

### TNZ（OpenToonz / Tahoma2D のシーン）

- 公開仕様は無い。形式を定めているのは OpenToonz のコード（[opentoonz](https://github.com/opentoonz/opentoonz) コミット a250456、2026-11-10）
  - 書き方：`toonz/sources/common/tstream/tstream.cpp`（`TOStream`）
  - 列・セル・レベル：`toonzlib/txshlevelcolumn.cpp`、`txshmeshcolumn.cpp`、`txsheet.cpp`、`txshsimplelevel.cpp`、`txshchildlevel.cpp`、`tstageobjecttree.cpp`、各列の `PERSIST_IDENTIFIER`
  - 解釈：`toonz/sources/toonz/xdtsio.cpp`（OpenToonz 自身の XDTS 書き出し）
- **実物**：OpenToonz の公式サンプルシーン [opentoonz_sample](https://github.com/opentoonz/opentoonz_sample)（コミット ff0c39d、OpenToonz 1.2.0 で保存）の `dwanko_run.tnz`、`cleanup.tnz`、`tga_paint.tnz`。ライセンスが CC BY-NC 4.0 なので**リポジトリには入れていない**

## 読み込み・保存

| 形式 | 読み込み | 保存 | 書き出しの基準 |
|---|---|---|---|
| XDTS | version 5 / 10。知らない version（実在する 11 など）も拒否しない | 5。カメラ位置があれば 10。p1 の印があれば `subversion: "p1"` | `csp_v5.xdts`（実物とバイト一致） |
| TDTS | version 5（直下の `timeTables`）、7 以降（`timeSheets`） | 読んだ version（7 以上）。それ以外は 10 | `saved_v10.tdts`（実物とバイト一致） |
| STS | 列数1バイト、コマ数・セルは u16。列名は UTF-8、それ以外は Shift_JIS（CP932） | u16・UTF-8。DiTiS が書くファイルとバイト一致 | DiTiS の `generateStsBuffer` |
| DiTiS | `version` / `sheets`。古い `{id, name}` と `data` を ID で引く形も | 新規シートは DiTiS と同じキー・順序・インデント2 | DiTiS の `buildDitisJson` |
| TNZ | トップ Xsheet のセル列（セルマークの印を含む）と台詞列 | **しない** | OpenToonz の `xdtsio.cpp` の解釈（1枚絵の番号だけ意図して `1`） |

### 共通の規則

- シートは**変化点の列**として持つ。コマとトラック番号は0始まり。値は次の変化点まで続く
- セルの値は**書かれた文字列のまま**持つ。`a0001`、`001`、`BG12` を数値に直さない（例外は TNZ のフレーム番号の桁埋めと STS の数値で、どちらもその形式の書き方による）
- 記号は `SYMBOL_NULL_CELL`（×）、`SYMBOL_TICK_1`（○）、`SYMBOL_TICK_2`（●）、`SYMBOL_HYPHEN`（前の指示の継続）
- **書けないものは別の値に置き換えない**。置き場所のない値は、その形式に書かない（XDTS の `episode`・`direction`、DiTiS の台詞・カメラ欄、STS の記号など。README の各節を参照）か、`Error::Unsupported` で断る（STS の `a0001` など）
- 知らないキーは捨てずに `Extra` に保ち、**読んだ形式にだけ**書き戻す（`Sheet::read_from`）。対象は XDTS・TDTS・DiTiS。DiTiS のシートでは、バイト単位で戻すためにモデルが持つキーも含めた全キーを順に保つ
- 公開仕様に FPS は無い。FPS を持つのは DiTiS だけで、これは DiTiS のキーとして保つ
- 形式の判別は**中身の先頭**で行い、拡張子は見ない（`from_bytes` / `from_str`）

## 検証と限界

### 実行するもの

| コマンド | 何を確かめるか |
|---|---|
| `cargo test` | `tests/formats.rs`（XDTS・TDTS：往復、実物とのバイト一致、キー構成、リビジョン、p1、知らないキー）、`tests/sts_tnz.rs`（STS：DiTiS 形・remaping 形の読み込み、往復、書けない値の拒否。TNZ：OpenToonz の書き出しと同じ解釈）、`tests/ditis.rs`（DiTiS が書いたファイルの往復・読み込み）、各モジュール内のテスト、README のコード例 |
| `cargo test --test json_oracle` | 同梱の JSON 読み書きを `serde_json`（テスト時だけの依存）と数千の文書で突き合わせる |
| `node tools/check-ditis.mjs /path/to/ditis` | DiTiS のチェックアウトから関数を取り出して Node で動かし、STS と DiTiS 独自形式をこのクレート（`cargo run --example convert`）と突き合わせる。8項目。下を参照 |
| CI（`.github/workflows/ci.yml`） | fmt・clippy・doc を警告なしで、`cargo test` を Linux と Windows で（fixture の改行がバイト単位で届くか）、`rust-version`（1.85）でのビルド |

`tools/check-ditis.mjs` の8項目は次のとおりです。DiTiS の関数は書き換えずに使い、アプリ側の表示や通知の関数だけを空にしています（`parseStsFile` は読んだシートをアプリに追加する最後の行だけ、返す形に変えています）。

1. DiTiS が書いた STS を、このクレートが読んで書き戻すとバイト単位で同じ
2. このクレートが書いた STS を DiTiS が読むと、DiTiS が書いたときのセルになる
3. DiTiS で作った新規シートの `.ditis` が、バイト単位で同じに戻る
4. その `.ditis` からこのクレートが書いた XDTS を DiTiS が取り込むと、同じセルになる
5. このクレートが `.ditis` から読む変化点が、DiTiS 自身の XDTS 書き出しの指示と同じ。往復だけでは、`○` を記号に直さず文字列のまま通す誤りが見つからない（その誤りを入れると 1〜4 は通り、5 だけが落ちることを確認した）
6. DiTiS が XDTS から取り込んだシートの `.ditis`（`timesheetSource` を持つ）が、バイト単位で同じに戻る
7. このクレートが XDTS から書いた `.ditis` を DiTiS が開くと、DiTiS が同じ XDTS を取り込んだときと同じセルになる
8. このクレートが TNZ から書いた `.ditis` を DiTiS が開ける

TNZ は `cargo run --example convert -- scene.tnz out.xdts` で手元のシーンを確かめられます。公式サンプル3本は、列の数・列名・各列の変化点が `xdtsio.cpp` の解釈どおりになることを確認しました（`dwanko_run.tnz` はエフェクト列が空の列として残ることも含めて）。

### 形式ごとの確かさ

| 形式 | 実物 | 第三者実装 | 合成 |
|---|---|---|---|
| XDTS | CLIP STUDIO PAINT EX の書き出し（v5）とバイト一致 | OpenToonz の `xdtsio.cpp`（p1） | `synthetic_v5.xdts`（読み込みのみ） |
| TDTS | 東映 DTS の保存シート（v10）とバイト一致 | remaping / DiTiS（リビジョン、欄4） | `synthetic_v7.tdts`（読み込みのみ）、手で組んだ欄4 |
| STS | — | DiTiS を実行（バイト一致）、remaping を読解 | remaping 形（u16・Shift_JIS）のファイル |
| DiTiS | DiTiS のコードを実行（新規シート・取り込んだシートともバイト一致） | — | 古い `{id, name}` 形のファイル |
| TNZ | 公式サンプル3本（OpenToonz 1.2.0、リポジトリ外） | OpenToonz のソースを読解 | `synthetic.tnz` |

### 限界

- **TDTS**：仕様書が無く、実物は1枚（1シート1リビジョン、欄0にセル）です。リビジョン履歴と欄4の扱いは第三者の記述と手で組んだファイルだけに基づいていて、完全な準拠とは言えません
- **XDTS**：実物との照合は CLIP STUDIO PAINT EX の version 5 です。アプリケーションが書いた version 10 のファイルは `tests/data/` にありません
- **STS**：元のアプリケーションも、それが書いたファイルも手元に無く、元のアプリとの互換は remaping の実装を読んだことだけが根拠です。Shift_JIS は CP932 として読み、変換表は Python の `cp932` コーデックから生成したものです（`python3 tools/gen-cp932.py` で再生成して一致を確かめられます）。256 コマ以上のシートは DiTiS では開けません
- **DiTiS**：動かしたのは DiTiS の関数で、アプリそのもの（Tauri の画面やファイル I/O）ではありません。バイト単位の往復は、DiTiS の新規シートと XDTS から取り込んだシートの2つの形で確かめています。セルを変えた後の `data` はモデルから書き直すので、空のコマの `×` と `""` の区別はそこで失われます（DiTiS にとっては同じ）。台詞・カメラ欄と p1 の印は書きません
- **TNZ**：OpenToonz を動かして書き出した XDTS との突き合わせはしていません。根拠はソースの読解と、OpenToonz 1.2.0 で保存されたサンプル3本です。新しい版や Tahoma2D に固有の保存内容（暗黙のホールド、ストップフレームなど）は確かめていません。セルマークは書き出しダイアログの既定の対応（`CellMarks::default()`）で読みますが、シートごとにマーク一覧を変えている場合は呼び出し側が対応を指定する必要があります。台詞列の並べ方は架空のサンプル1つに倣ったもので、CLIP STUDIO PAINT などで台詞として表示されるかは確かめていません。サブ Xsheet の中身と FPS は読みません。書き出しはしません
- **テストデータ**：`tests/data/` の基準ファイルは、実物の形を写して中身を架空にしたものか、アプリケーションのコードに書かせたものです。制作シートそのものは入れていません。それぞれの出どころは NOTICE にあります
