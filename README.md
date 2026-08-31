# FUIDE — FUI Develop Environment

Sci-Fi / FUI (Futuristic UI) デザインのアプリを作るための開発環境。中核は egui (0.36) 向けの `fuide` クレート (テーマ・窓シェル・部品) で、その上に最初のアプリとしてファイルマネージャーと Homebrew フロントエンド (macOS デスクトップ) を載せています。今後はモバイルなどデスクトップ以外のアプリも同じ基盤で作る予定です。

```
crates/fuide/        FUI 部品ライブラリ `fuide` (egui のみ依存)
  theme.rs           パレット CYAN / AMBER / GREEN、フォント登録、ウィジェットスタイル
  shell.rs           フレームレス窓シェル (直角の枠、グロー、タイトル/ステータスバー、リサイズ)
  panel.rs           タイトルチップ付きパネル
  widgets.rs         ナビタブ、ボタン、セグメントバー、円弧ゲージ、ランプ、ログフィード、読み出し行
  fx.rs              走査線、走査帯
  geom.rs            多角形、グロー描画 (チャンファーはオプション)
apps/file-manager/       FUIDE File Manager — Finder 風ファイルブラウザ (macOS)
apps/brew/           FUIDE Brew — Homebrew の GUI (brew info --json / search / streaming runner)
assets/fonts/        Orbitron (見出し) / Share Tech Mono (データ) — いずれも OFL
```

## FUIDE File Manager

```sh
cargo run -p fuide-file-manager            # $HOME から開始
cargo run -p fuide-file-manager -- /usr/bin
cargo test -- --ignored trash            # 実際にゴミ箱へ移動する統合テスト (捨てファイルを 1 つ ~/.Trash に残す)
```

- 左: よく使う場所 + `/Volumes` のボリューム、ストレージ使用率ゲージ
- 中央: 履歴ボタン、パンくず (クリックで移動)、フィルター、隠しファイル切替、ファイル一覧 (列ヘッダでソート)
- 右: 選択項目のインスペクター (種類 / サイズ / 日時 / パーミッション / パス)、OPEN / FINDER / COPY / RENAME / DELETE
- エラー (削除失敗 / ディレクトリ読取拒否 / OS で開けない) は大きな `ERROR` カード (`fuide::dialog::alert`) で通知。カードには操作名だけ、詳細はイベントログ。Enter / Space / Esc / ACKNOWLEDGE で閉じる。複数のエラーは順番に表示
- リネームと削除は FUI 風のモーダルダイアログ (`fuide::Dialog`、開閉とも 0.15 秒のフェード)。削除は既定でゴミ箱へ移動 (`NSFileManager.trashItem`、Finder から復元可)、ダイアログ内の PERMANENT で完全削除に切替 (枠が危険色になる)
- 下: イベントログ (長い行は折り返し、ドラッグ選択して Cmd+C でコピー可、溢れたらスクロール)、ステータスバー (`T+HH:MM:SS.s` 経過時間、24h 超で `T+1d …`、件数、FPS、FS リンク状態)

| 操作 | キー |
|---|---|
| 選択移動 / 開く / 親へ | ↑↓ / Enter / Backspace |
| 履歴 戻る / 進む | Cmd+[ / Cmd+]、マウスの戻る / 進むボタン |
| リネーム | Cmd+R (ダイアログ。Enter 確定 / Esc 取消) |
| ゴミ箱へ移動 / 完全削除 | Cmd+Backspace / Cmd+Option+Backspace (確認ダイアログ) |
| フィルターにフォーカス / クリア | Cmd+F / Esc |
| パレット切替 CYAN / AMBER / GREEN | Cmd+1 / 2 / 3 |
| ダブルクリック | ディレクトリは移動、ファイル・.app は OS で開く |

日本語ファイル名は起動時に `/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc` をフォールバックとして読み込んで表示する (無ければスキップ)。egui はフォールバック書体を行高の差の分だけずらして置くため (ヒラギノは lineGap 0.5em で約 0.19em 浮く)、`fuide::fontmetrics` が hhea / OS/2 を読んで主書体ごとに `y_offset_factor` を計算し、Share Tech Mono 用と Orbitron 用の 2 通りで登録している。

### 開発用スクリーンショット

macOS の画面収録権限が無い端末からでも見た目を確認できるよう、アプリ自身で撮影できる:

```sh
FUIDE_SCREENSHOT=/path/shot.tga cargo run -p fuide-file-manager   # 45 フレーム後に撮影して終了
FUIDE_DEV_DIALOG=rename|trash|delete|error ...                   # 先頭項目でダイアログを開いた状態で撮影
FUIDE_DEV_LOG="long error text" ...                        # 起動時にログへ赤い行を 1 本入れる (折り返し確認用)
FUIDE_DEV_DIALOG_CLOSE=45 FUIDE_SCREENSHOT_FRAME=50 ...      # 45F でダイアログを閉じ、50F で撮影 (フェードアウト確認用)
sips -s format png /path/shot.tga --out /path/shot.png
```

## FUIDE Brew

```sh
cargo run -p fuide-brew
```

- 左: ビュー (INSTALLED / OUTDATED / CASKS / SEARCH、Cmd+1..4)、SYSTEM パネル (最新率ゲージ、formulae / casks / outdated / pinned、Cellar・Caskroom 容量、最終 `brew update`)
- 中央: リロード、UPDATE、UPGRADE ALL (n)、フィルター (Cmd+F)、一覧 (NAME / VERSION / LATEST / KIND / STATUS、列ソート)。SEARCH ビューでは検索欄 (formulae と casks を検索し、上位 25 件ずつ `brew info` で詳細取得)
- 右: パッケージ詳細 (説明、状態、版、tap、ライセンス、導入日、依存、ホームページ、caveats) と HOMEPAGE / COPY / PIN / UPGRADE / UNINSTALL / INSTALL
- 下: brew の標準出力・標準エラーをストリーミング表示 (`==>` = 緑、`Warning` = 注意色、`Error` = 危険色)
- 変更系 (update / upgrade / install / uninstall) は確認ダイアログ → 別スレッドで実行、完了後に在庫を再取得。成功は SUCCESS カード、失敗は ERROR カード。同時実行は 1 つ
- 読み取り系は `HOMEBREW_NO_AUTO_UPDATE=1` で呼ぶ (自動更新で数秒待たされないため)。全コマンドに `NONINTERACTIVE=1`、stdin は閉じるので sudo 待ちで固まらない
- キー: ↑↓ 選択、Enter ホームページ、Cmd+Backspace アンインストール、Cmd+R 再取得
- 撮影フック: `FUIDE_DEV_DIALOG=uninstall|upgrade|error|success`、`FUIDE_DEV_RUN="doctor"` (起動時に brew コマンドを流す)、`FUIDE_DEV_SEARCH=ripgrep`

## 再描画レートとウィンドウマネージャー

シェルのアイドルアニメーション（枠のパルス・走査帯）は **20 fps** で再描画する（`fuide::shell::IDLE_FPS`、環境変数 `FUIDE_IDLE_FPS` で変更、`0` = 毎フレーム）。毎フレーム再描画すると macOS では Rectangle などのスナップ操作で 200〜500 ms 遅れる（[winit #3644](https://github.com/rust-windowing/winit/issues/3644)、[kobago/fuide#1](https://github.com/kobago/fuide/issues/1)）。ダイアログのフェードや brew 出力の流入など一時的なアニメーションは従来どおり即時に再描画する。`FUIDE_DEV_FRAMELOG=1` で 30 フレームごとの時刻を stderr に出せる。

## 配布 (.app / DMG、Apple Silicon)

```sh
cargo install cargo-bundle          # 初回のみ
./scripts/release.sh                # dist/FUIDE File Manager.{app,dmg}, dist/FUIDE Brew.{app,dmg}
./scripts/release.sh fuide-brew       # 1 本だけ
```

- `cargo bundle --format osx` で `.app`（`Info.plist`、`assets/icons/*.svg` から `.icns`）→ `codesign`（既定は ad-hoc）→ `hdiutil` で `/Applications` へのリンク入り DMG
- バンドル設定は各 `apps/*/Cargo.toml` の `[package.metadata.bundle]`（識別子 `fuide.file-manager` / `fuide.brew`、最小 macOS 13）。`icon` のパスは cargo-bundle を実行したディレクトリ基準なので、スクリプトはワークスペース root で実行する
- Spotlight から起動するには DMG を開いて `.app` を `/Applications` にドラッグ（インデックスに数十秒。急ぐなら `mdimport /Applications/FUI\ Brew.app`）
- **他の Mac に配る場合**: Developer ID で署名・公証していないので、受け取った側は初回だけ右クリック → 開く、または `xattr -d com.apple.quarantine "/Applications/FUIDE Brew.app"` が必要。Developer ID を取得したら `SIGN_IDENTITY="Developer ID Application: ..." ./scripts/release.sh` で署名し、`xcrun notarytool submit dist/*.dmg --wait` → `xcrun stapler staple` で公証
- FUIDE Brew は launchd 起動の最小 `PATH` でも動くよう `brew` を `/opt/homebrew/bin` → `/usr/local/bin` → `PATH` の順で探す

## ターミナルから開く (`open` 風)

```sh
./scripts/install-cli.sh            # /opt/homebrew/bin (書込可なら) or ~/.local/bin に ffm / fuide-brew を置く
ffm                                 # カレントディレクトリを開く
ffm ~/Downloads                     # 指定ディレクトリを開く (相対パス可)
fuide-brew
```

- `ffm` は `open -na "FUIDE File Manager" --args <絶対パス>` を呼ぶだけ。LaunchServices 経由なので Dock に出て、ターミナルを閉じても残る。`-n` で毎回新しいウィンドウ（プロセス）が開く
- `open` は起動先の cwd を `/` にするため、ラッパー側で `cd "$dir" && pwd -P` で絶対化してから渡している
- アプリは `/Applications` か `~/Applications` に入れておく（DMG からドラッグ）。`open` は LaunchServices のデータベースからバンドル名で探すので、パスは不要

## `fuide` クレートの使い方 (最小)

```rust
fn new(cc: &eframe::CreationContext<'_>) -> Self {
    fuide::theme::install(&cc.egui_ctx, fuide::Palette::cyan(), vec![]);
    ..
}
fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
    fuide::Shell::new("My Tool").subtitle("v0.1").lamp("LINK OK", pal.ok, false)
        .show(ui, |ui| {
            fuide::Panel::new("Telemetry").show_rect(ui, rect, |ui| { .. });
        });
}
```

`NativeOptions.viewport` は `with_decorations(false).with_transparent(true)`、`App::clear_color` は `[0.0; 4]` にする。

文字サイズは `fuide::TypeScale` に集約 (既定 `NORMAL`: 本文 13.5px / ラベル・見出し 13px / 脚注 12px / 行高 24px、Finder の 13px 相当)。密度を上げたいときは `theme::set_type_scale(&ctx, TypeScale::COMPACT)` か `.scaled(f)`。egui 標準の Cmd +/- でも全体をズームできる。

角は既定で直角。45° のチャンファーが欲しいときだけ `fuide::theme::set_corners(&ctx, fuide::Corners::CHAMFER)` を呼ぶ (窓 26 / パネル 14 / タブ 12 / ボタン 7 px)。
