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
| 設定ウィンドウ | Cmd+, またはタイトルバーの歯車 |
| 終了 | Cmd+W (ウィンドウを閉じる = アプリ終了。ダイアログ中や入力欄フォーカス中でも効く) |
| ダブルクリック | ディレクトリは移動、ファイル・.app は OS で開く |

日本語ファイル名は起動時に `/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc` をフォールバックとして読み込んで表示する (無ければスキップ)。egui はフォールバック書体を行高の差の分だけずらして置くため (ヒラギノは lineGap 0.5em で約 0.19em 浮く)、`fuide::fontmetrics` が hhea / OS/2 を読んで主書体ごとに `y_offset_factor` を計算し、Share Tech Mono 用と Orbitron 用の 2 通りで登録している。

### 開発用スクリーンショット

macOS の画面収録権限が無い端末からでも見た目を確認できるよう、アプリ自身で撮影できる:

```sh
FUIDE_SCREENSHOT=/path/shot.tga cargo run -p fuide-file-manager   # 45 フレーム後に撮影して終了
FUIDE_DEV_DIALOG=rename|trash|delete|error ...                   # 先頭項目でダイアログを開いた状態で撮影
FUIDE_DEV_LOG="long error text" ...                        # 起動時にログへ赤い行を 1 本入れる (折り返し確認用)
FUIDE_DEV_DIALOG_CLOSE=45 FUIDE_SCREENSHOT_FRAME=50 ...      # 45F でダイアログを閉じ、50F で撮影 (フェードアウト確認用)
FUIDE_DEV_SETTINGS=1 FUIDE_DEV_EMBED=1 FUIDE_CONFIG_DIR=/tmp/cfg ...  # 設定ウィンドウを開いた状態で撮影 (本体に埋め込む。設定ファイルは /tmp/cfg に隔離)
FUIDE_DEV_TRACE=1 ...                                      # eframe / egui の log 出力 (再描画スケジュール、viewport の生成/破棄) を stderr へ
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
- キー: ↑↓ 選択、Enter ホームページ、Cmd+Backspace アンインストール、Cmd+R 再取得、Cmd+, 設定、Cmd+W 終了 (実行中の brew コマンドは止めない: 子プロセスはそのまま完走する)
- 撮影フック: `FUIDE_DEV_DIALOG=uninstall|upgrade|error|success`、`FUIDE_DEV_RUN="doctor"` (起動時に brew コマンドを流す)、`FUIDE_DEV_SEARCH=ripgrep`
- `FUIDE_BREW_BIN=/path/to/brew` で呼び出す `brew` を差し替えられる (テストは `fixtures/fake-brew.sh` を使う。`FUIDE_FAKE_BREW_LOG` にコールを記録)

## 設定ウィンドウ (テーマ)

両アプリとも `Cmd+,` かタイトルバーの歯車で設定ウィンドウが開く。パレット (CYAN / AMBER / GREEN)、角 (SQUARE / CHAMFER)、密度 (NORMAL / COMPACT) を選ぶと即座に本体へ反映され、ファイルに保存される。閉じるのは × / Esc / Cmd+W。

- 設定ウィンドウは egui の **子 viewport** (別のネイティブウィンドウ、`show_viewport_deferred`) で、本体と同じ `fuide::Shell` を `tool_window()` (閉じるボタンのみ・リサイズなし・アイドルアニメ無し = 入力があったときだけ再描画) で描いている。フォントや Visuals は `egui::Context` 全体で共有なので、子ウィンドウで変えた瞬間に本体も変わる
- 子 viewport は eframe 0.36 では撮影できない (immediate は `Screenshot` コマンドを捨てる。deferred は macOS でイベントループが約 1 秒止まったあと再描画が来なくなる)。撮影は `FUIDE_DEV_EMBED=1` で本体に埋め込んで行う (上の「開発用スクリーンショット」)
- 保存先は macOS では `~/Library/Application Support/FUIDE/<app>.conf` (`file-manager.conf` / `brew.conf`)、他 OS では `$XDG_CONFIG_HOME/fuide/` か `~/.config/fuide/`。`FUIDE_CONFIG_DIR` で置き換え可。中身は `palette=amber` のような `key=value` 行で、知らないキーは無視、足りないキーは既定値
- 自作アプリで使うには `fuide::Settings` と `fuide::SettingsWindow` (下の「クレートの使い方」参照)

## テスト

```sh
cargo test                                   # 単体 + UI テスト (オフラインで完結、数秒)
cargo test -- --ignored                      # 実機依存 (Finder のゴミ箱など)
UPDATE_SNAPSHOTS=true cargo test -p fuide    # 見た目が意図的に変わったときにスナップショットを更新
```

| 層 | 場所 | 中身 |
|---|---|---|
| 単体 (fuide) | 各モジュールの `#[cfg(test)]` | `fmt` / `fontmetrics` / `settings` の純関数 |
| UI (fuide) | `crates/fuide/tests/ui.rs` | [`egui_kittest`](https://docs.rs/egui_kittest) でシェル + パネル + 部品をヘッドレス描画。**アクセシビリティ木**でボタンやタブをラベルから探してクリック・状態確認、**wgpu スナップショット** (`tests/snapshots/*.png`、`kittest.toml` の閾値) で見た目の回帰を検出 |
| 単体 (アプリ) | `apps/*/src/*.rs` | `fs.rs` / `brew.rs` の純関数 |
| 状態機械 (アプリ) | `apps/*/src/app/tests.rs` | `Explorer::with_context(ctx, dir, settings)` / `BrewApp::with_context(ctx, settings)` で `CreationContext` 無しにアプリを作り、`Action` を適用して状態・ログ・ダイアログを検証。ファイルマネージャーは一時ディレクトリで実ファイル操作 (一覧・ソート・フィルター・履歴・リネーム・完全削除・読取拒否) まで通す。ローダーやファイル操作のスレッドは `ui()` と同じく `poll_*` を回して待つ |
| E2E (アプリ) | `apps/*/src/app/e2e.rs` | `egui_kittest` の `Harness::new_eframe` で本物の `Explorer` / `BrewApp` を起動し、アクセシビリティ木からラベルでクリック・キー入力・文字入力して状態を検証。ファイルマネージャー: 行クリック → Enter で移動 / Backspace / Cmd+[ ] / 矢印、Cmd+F → 入力 → Esc、歯車 → パレット・角の変更が保存される。brew (偽 brew): ビュー切替 (タブ / Cmd+数字)、UPGRADE ALL → 確認 → 出力ストリーム → SUCCESS カード → ACKNOWLEDGE、検索ビューで Cmd+F → 入力 → Enter、Cmd+, → パレット保存。設定ウィンドウは kittest では埋め込み `egui::Window` になる |
| 結合 (brew) | 同上 + `apps/brew/fixtures/` | `FUIDE_BREW_BIN` を `fixtures/fake-brew.sh` に向け、本物の worker スレッドとストリーミング実行 (`==>` 行のログ流入、成功/失敗カード、完了後の在庫再取得、検索結果への導入状態の反映) を Homebrew 無しで検証。`info-installed.json` が在庫のフィクスチャ |

決めごと:
- 実機依存 (Finder のゴミ箱、本物の brew、画面収録) は `#[ignore]` か偽物に差し替え、`cargo test` はオフラインで通す
- **操作できる部品は必ず `Response::widget_info` でラベルを持つ** (`nav_tab` / `button` / `icon_button` / `toggle_chip` / テーブルの列見出し・行 / シェルの窓ボタンと歯車)。ラベルは**描画と同じ大文字**にする。アイコンだけのボタンは `Icon::label()` (`REFRESH` など)。これが UI テストと支援技術の共通の入口
- 状態は egui の流儀で読む: `WidgetInfo::selected` は AccessKit の `toggled` に写るので、テストでは `node.accesskit_node().toggled() == Some(Toggled::True)`
- シェルは常時アニメして毎フレーム再描画を要求するので、kittest では `run()` (静止待ち) ではなく `run_steps(n)` + `with_step_dt` で決定的に進める
- ハーネスは生成時に最初のフレームを回すため、フォント登録 (`theme::install`) は最初のフレームで行い、そのフレームは何も描かない (`set_fonts` は次パスから有効)
- ダイアログはフェードインの最初のフレーム (opacity 0) では部品が無効 (egui は不可視の `Ui` を disable する) なので、E2E では `!accesskit_node().is_disabled()` になるまで待ってからクリックする
- 同じ文字列が複数の場所に出るとき (選択した行の名前がインスペクターにも出る等) は `get_by_role_and_label(Role::Button, ..)` で絞る
- E2E が見つけた実バグ: egui は Esc でフォーカスを先に外すので `has_focus()` では Esc を拾えない → `lost_focus()` も見る (フィルターの Esc クリアが動いていなかった)。brew の検索ビューでは `Cmd+F` を検索欄に向ける

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

## 他のプロジェクトから `fuide` を使う (git 依存)

`fuide` はまだ crates.io には公開していないので、GitHub の URL を `Cargo.toml` に書いて取り込む。ワークスペース内の `crates/fuide` は Cargo がパッケージ名で見つけるので、パスの指定は不要。

```toml
[dependencies]
egui = "0.36.1"
eframe = { version = "0.36.1", default-features = false, features = ["default_fonts", "wgpu"] }
fuide = { git = "https://github.com/kobago/fuide" }
```

- 再現性のため、コミットかタグで固定するのを推奨: `{ git = "...", rev = "65ca5ba" }` / `{ git = "...", tag = "v0.1.0" }`。`branch = "main"` で追従もできる。何も書かなくても `Cargo.lock` にコミットが記録され、`cargo update` で進む
- リポジトリが private の間は認証が要る。SSH が簡単: `fuide = { git = "ssh://git@github.com/kobago/fuide" }`。HTTPS を使うなら `~/.cargo/config.toml` に `[net] git-fetch-with-cli = true` を入れてシステムの git (credential helper) に任せる
- `egui` / `eframe` は `fuide` と同じ 0.36 系に揃える (ずれると型が一致せずコンパイルできない)
- crates.io に公開したら `fuide = "0.1"` に差し替えるだけで移行できる

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

`NativeOptions.viewport` は `with_decorations(false).with_transparent(true)`、`App::clear_color` は `[0.0; 4]` にする。`Shell` は Cmd+W で自分のウィンドウに `ViewportCommand::Close` を送る (本体なら終了、`tool_window()` は自前で閉じる)。

設定ウィンドウを付けるなら、起動時に `Settings::load("my-tool")` で読んで `install` に渡し、毎フレームの最後に `SettingsWindow::show` を呼ぶ:

```rust
let settings = fuide::Settings::load("my-tool").unwrap_or_else(|| fuide::Settings::new(fuide::PaletteKind::Cyan));
fuide::theme::install(&cc.egui_ctx, settings.palette.palette(), vec![]);
settings.apply(&cc.egui_ctx);
..
let out = fuide::Shell::new("My Tool").settings_button(true).show_full(ui, |ui| { .. });
if out.settings_clicked { self.settings_win.open(); }
if self.settings_win.show(ui.ctx(), &mut self.settings, "My Tool") {
    self.settings.save("my-tool").ok();   // 変更があったフレームだけ true
}
```

文字サイズは `fuide::TypeScale` に集約 (既定 `NORMAL`: 本文 13.5px / ラベル・見出し 13px / 脚注 12px / 行高 24px、Finder の 13px 相当)。密度を上げたいときは `theme::set_type_scale(&ctx, TypeScale::COMPACT)` か `.scaled(f)`。egui 標準の Cmd +/- でも全体をズームできる。

角は既定で直角。45° のチャンファーが欲しいときだけ `fuide::theme::set_corners(&ctx, fuide::Corners::CHAMFER)` を呼ぶ (窓 26 / パネル 14 / タブ 12 / ボタン 7 px)。
