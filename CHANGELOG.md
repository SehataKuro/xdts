# 変更履歴

## 未リリース

`Format` に値を足したので、次は **0.2.0** です。

### 互換性のない変更

- `Format` に `Ditis` を追加し、`#[non_exhaustive]` を付けました。`Format` を `match` している場合は `_` の腕が必要です

### 追加

- **DiTiS** の独自形式（`.ditis` / `.json`）の読み書き：`ditis::from_str` / `ditis::to_string`。モデルに無いもの（FPS、挿入・無効行、取り込み元の文書など）も保って書き戻し、DiTiS が書いた新規シートも XDTS から取り込んだシートもバイト単位で往復します
- **STS**（シラヘイタイムシート、DiTiS）の読み書き：`sts::from_bytes` / `sts::to_bytes`。DiTiS が書くファイルとはバイト単位で往復し、元のアプリの読み方（256 コマ以上、漢字を含む Shift_JIS（CP932）の列名）も読めます
- **TNZ**（OpenToonz / Tahoma2D のシーン）の読み込み：`tnz::from_str` / `tnz::from_bytes`。OpenToonz 自身の XDTS 書き出しと同じ解釈でセル欄を取り出し、セルマークから中割・原画の印を（`tnz::CellMarks`、`tnz::from_str_with`）、台詞列から台詞欄を読みます。番号のない1枚絵だけは OpenToonz の `"-2"` ではなく `1` にします
- `from_bytes`：STS・XDTS・TDTS・DiTiS・TNZ を中身の先頭で判別して読みます。`from_str` も DiTiS と TNZ を判別します
- `examples/convert.rs`：読める形式から書ける形式へ変換する例（`cargo run --example convert -- in.tnz out.xdts`）
- `FORMATS.md`：形式ごとの根拠・確かめたこと・限界。`tools/check-ditis.mjs` で DiTiS のコードとの突き合わせを再現できます（パッケージには含めません）
- `Error::Unsupported`：書き出す形式に置き場所がないものを、別の値に置き換えずに断るためのエラー

## 0.1.0

初めての公開版です。

### できること

- **XDTS** の読み書き。version 5（2018-11-29）と version 10（2022-12-06）の両方に対応し、書き出す version はシートの中身から決まります（カメラ位置があれば 10、なければ 5）
- **TDTS** の読み書き。アプリケーションが保存したシートを読んで書き戻すと、バイト単位で同じファイルになります
- セル欄・台詞欄・カメラワーク欄の3つすべて、リビジョン履歴、作業伝票、文字色、ダミーコマ、非公式拡張 p1（原画・中割参考のマーク）
- このクレートが知らないキーも捨てずに保ち、読んだ形式に書き戻します
- 依存クレートはゼロです。JSON の読み書きは同梱していて、テストのときだけ `serde_json` と突き合わせて検証しています

対応する Rust は 1.85 以降です。

### git のリビジョンから移行する場合

公開前に `git = "https://github.com/SehataKuro/xdts"` で使っていた場合は、次の変更があります。

**API**

- `Track::frames` の値が `String` から `Cell` になりました。`"1".into()` はそのまま使えます。そのコマの値だけが欲しいときは `Track::value_at` を使います
- `ValueTrack::frames` の値が `Vec<String>` から `Instruction` になりました。`["A", "おはよう"].into()` で作れます。`ValueTrack::at` は今までどおり値の並びを返します
- `Sheet`・`TimeTable`・`WorkSlip` にフィールドが増えました。構造体リテラルには `..Default::default()` を付けてください
- `Sheet` と `TimeTable` は `PartialEq` のみで、`Eq` ではなくなりました（保持する未知のキーが JSON の値で、浮動小数点を含むため）
- `Error` に `#[non_exhaustive]` を付けました。`match` には `_` の腕が必要です

**書き出し**

- TDTS は version 10・LF・インデント4で書きます。以前の版は version 7・CRLF・インデント2でした
- TDTS のヘッダのキーは `showHeadDummy` です。以前の版は存在しないキー `showHeaderDummy` を書いていました
- TDTS の `books` は書かなくなり、`free`・`headDummykomas`・`footDummykomas` を書くようになりました
- XDTS のキーは、ファイルが持っている順（`timeTables`、`version`、`header`、中身の後に番号）で書きます。以前の版はアルファベット順でした
- `cut` と `scene` は書かれた値をそのまま書きます。以前の版は数字だけに削っていました（`C-112` → `112`）。数字だけにしたい場合は `xdts::header_number` を使ってください

**読み込み**

- JSON は RFC 8259 に沿って厳密に読みます。`+1`、`01`、存在しないエスケープ、文字列中の生の制御文字は拒否します
- 入れ子が128段を超えるファイルはエラーになります。以前の版は1,000段ほどでスタックを使い果たし、プロセスごと落ちていました
