# FUIDE CAD

[FUIDE](https://github.com/kobago/fuide) (Sci-Fi / FUI デザインの egui 部品ライブラリ `fuide`) の上に載せたパラメトリック 3D CAD (macOS)。

```
apps/cad/            FUIDE CAD — Manifold のメッシュカーネル + truck、フィーチャー列 + 式、ねじ山、STL / JSON、MCP の CAD 専用ツール
crates/fuide-3d/     3D ビューポート `fuide-3d` (wgpu): Z-up オービットカメラ、ホログラム塗り + グローする線、egui ウィジェット
assets/icons/        .app のアイコン (SVG)
scripts/             release.sh (.app / DMG)、install-cli.sh (ターミナル用ランチャー)
```

`fuide` は git 依存 (`Cargo.toml` の `[workspace.dependencies]`)。`fuide` クレート自体を隣の `../fuide` で直しながら動かすときは `Cargo.toml` 末尾のコメントの `[patch]` を外す。

共通の仕組み (設定ウィンドウ、MCP エージェント、テストの決めごと、再描画レート) は [kobago/fuide](https://github.com/kobago/fuide) の README を参照。

## FUIDE CAD

```sh
cargo run -p fuide-cad                      # 空のドキュメント
cargo run -p fuide-cad -- bracket.cad.json  # ドキュメントを開く
FUIDE_DEV_SAMPLE=1 cargo run -p fuide-cad   # サンプル (穴あきブラケット) を読み込んで起動
```

小型ロボットの部品を個人で手軽に設計するための CAD ([#5](https://github.com/kobago/fuide/issues/5))。**マウスで線を引く CAD ではなく、フィーチャー列 (操作履歴) とパラメータを編集する CAD** で、GUI からもテキスト (JSON / MCP) からも同じ列を編集する。単位は mm、値はすべて式 (`w / 2 + 3`、`sqrt` / `sin` / `min` …、パラメータ名を参照できる)。

カーネルは 2 つのハイブリッド (`apps/cad/src/mesh.rs` と `kernel.rs`、どちらも GUI 無しでテストできる):

- **モデリングと表示は Manifold** ([manifold-rust](https://github.com/larsbrubaker/manifold-rust)、OpenSCAD が採用したメッシュブーリアンの純 Rust 移植、Apache-2.0、git 依存で rev 固定)。形状は閉じた三角形メッシュで、ブーリアンは厳密で失敗しない (共平面の面も、稜線を通る円柱も可)。曲面は弦公差 (0.02 mm) から決めた分割数の多角形。稜線は隣接三角形の二面角 (30° 超) から拾う。**ねじ山**はらせんの V 断面を (角度, 高さ) の高さ場として直接メッシュ生成する (`THREAD` フィーチャー)
- **truck** ([ricosjp/truck](https://github.com/ricosjp/truck)、Rust 製 B-rep カーネル、master を rev 固定) は STEP の入出力のために残してある (未接続)。B-rep でのモデリングは `kernel.rs` に実装とテストが揃っているが、ブーリアンが自由曲面や共平面に弱く、らせん掃引が無いので、モデリングの主役からは外した。切り替え時の知見は下に残す

- **左上: FEATURES** — フィーチャー列 (NAME / KIND / STATE / #)。行クリックで選択、ダブルクリックで抑制 (SUPPRESS) の切替。STATE は `BODY` (結果の実体) / `USED` (後のフィーチャーに消費された) / `ERROR` / `OFF`
- **左下: PARAMETERS** — 名前 = 式 の一覧 (右に評価値)。`×` で削除、下の NAME / VALUE + ADD で追加
- **中央: ツールバー 2 段 + VIEWPORT** — 1 段目 `ADD BOX / CYLINDER / THREAD`、`UNION / CUT / INTERSECT` (選択中のフィーチャーを A にして、次にクリックした行が B。ESC で取消)、`MOVE / ROTATE` (選択中のフィーチャーを消費する変換を追加)。2 段目 `ISO / FRONT / TOP / RIGHT / FIT` と表示モード `SHADED / WIRE / X-RAY`。ビューポートはドラッグでオービット、Shift+ドラッグ (または右 / 中ボタン) でパン、ホイールでズーム、ダブルクリックで FIT。結果の実体をホログラム塗り + 稜線のグローで描き、選択中の実体は稜線が注意色になる。XY 平面のグリッドと XYZ 軸 (赤 / 緑 / アクセント)、左下に三軸のトライアド
- **右上: SELECTED** — 選択中のフィーチャーの名前 (編集可)、入力 (`#3 BODY // #4 MOUNT HOLE`)、各フィールドの式の入力欄 (`ORIGIN.X` … 打ち替えると即再評価)、AXIS チップ、状態、SUPPRESS / REMOVE (後のフィーチャーが使っていれば拒否)
- **右下: MEASURE** — 選択中 (無ければ最初) の実体の体積 (cm³)、寸法、最小点、重心、三角形数、稜線数
- **下: イベントログ**、ステータスバー (`KERNEL` ランプは評価中に点滅、エラー数、`AGENT`)

| 操作 | キー |
|---|---|
| 選択移動 / 解除 | ↑↓ / Esc |
| 視点 / フィット | 1 (ISO) 2 (FRONT) 3 (TOP) 4 (RIGHT) / F |
| 取り消し / やり直し | Cmd+Z / Cmd+Shift+Z (100 段。同じ欄の連続編集は 2 秒以内なら 1 段) |
| 開く | Cmd+O: **macOS のファイルダイアログ** (`NSOpenPanel`、`.json` のみ)。Cmd+L: アプリ内のパス入力ダイアログ (`~` と相対パス、Tab 補完。**MCP エージェントはこちら**、macOS のダイアログの中は見えない) |
| 新規 / 保存 / STL 書き出し | Cmd+N / Cmd+S / Cmd+E (保存と書き出しはアプリ内のパス入力ダイアログ。既存ファイルへの上書きは確認ダイアログで、エージェントは人間留保) |
| フィーチャーを削除 | Cmd+Backspace |
| 設定 / 終了 | Cmd+, / Cmd+W |

ファイルは JSON (`*.cad.json`): `params` と `features` の列。フィーチャーは `box {origin, size}` / `cylinder {base, axis, radius, height}` / `thread {base, axis, diameter, pitch, length}` (ISO 風の外ねじ。頭や軸芯と UNION する) / `boolean {op, a, b}` / `translate {target, by}` / `rotate {target, origin, axis, angle}`。`a` / `b` / `target` は先のフィーチャーの id で、**参照されたフィーチャーは消費される**: 後のフィーチャーに消費されていない実体が結果 (複数あってよい)。STL は結果の実体をまとめてバイナリで書く。

truck を B-rep モデリングに使っていたときに分かった癖と対処 (`kernel.rs` に残っている):

- ブーリアンの公差は部品寸法の約 1 % が安定。細かすぎると `None` か内部 panic。結果の三角形化はメッシュ公差との組み合わせで panic するので、ブーリアン公差 × ナッジ × メッシュ公差を一緒に探索し、面が全部揃って三角形化できた候補だけ採用する
- **共平面の面同士は交差計算できない** (`This wire is not simple`): 工具側を重心まわりに 0.9999 / 1.0001 倍して再試行する。角の稜線を円柱が通る退化配置は失敗する
- カーネルの panic は `catch_unwind` でエラーに変え、フィーチャーを `ERROR` にして続行する (Manifold でも同じ守りを掛けている)。評価は別スレッドで、編集中は前の実体を表示し続ける
- 面と三角形の順序が並列イテレーターで実行ごとに変わる (重心でソートして固定)。フィレット / チャンファーは無い。らせん掃引が無いのでねじ山は作れない → Manifold へ

MCP: 汎用の `observe` / `click` / `type` に加えて **CAD 専用ツール** がある (下の「AI エージェントから操作する」)。`document` (JSON 全体)、`add_feature` (JSON のフィーチャーをそのまま渡す。`thread` も可、`union` / `cut` / `intersect` は `boolean` の略記)、`set_field` (`size.z` / `axis` / `name` / `suppressed`)、`remove_feature`、`set_param` / `remove_param`、`select`、`measure` (体積・寸法・重心・エラー)、`view` (視点 / モード / フィット)、`export` (STL / JSON。既存ファイルへの上書きは人間留保)、`open` (`new: true` で新規)。各ツールは通常の操作と同じ経路 (ログ、取り消し) を通り、結果の文の後に観測が付く。

撮影フック: `FUIDE_DEV_SAMPLE=1`、`FUIDE_DEV_DIALOG=open|save|overwrite|error`。

## AI エージェントから操作する (MCP)

各アプリは MCP サーバーを内蔵している。設定ウィンドウ (`Cmd+,`) の AGENT パネルで `ON` にすると Unix ソケットで待ち受け、Claude Code などの MCP クライアントが画面を読み・クリックし・文字を打てる。ツールと仕組みは fuide の README。

```sh
claude mcp add fuide-cad -- "/Applications/FUIDE CAD.app/Contents/MacOS/fuide-cad" --mcp
# 開発中は cargo のバイナリでも同じ
claude mcp add fuide-cad -- target/debug/fuide-cad --mcp
```

## テスト

```sh
cargo test                                   # 単体 + UI テスト (オフラインで完結、数秒)
cargo test -- --ignored                      # 実機依存 (Finder のゴミ箱など)
UPDATE_SNAPSHOTS=true cargo test -p fuide-cad  # 見た目が意図的に変わったときにスナップショットを更新
```

| 層 | 場所 | 中身 |
|---|---|---|
| 状態機械 (アプリ) | `apps/*/src/app/tests.rs` | `Explorer::with_context(ctx, dir, settings)` / `BrewApp::with_context(ctx, settings)` で `CreationContext` 無しにアプリを作り、`Action` を適用して状態・ログ・ダイアログを検証。ファイルマネージャーは一時ディレクトリで実ファイル操作 (一覧・ソート・フィルター・履歴・リネーム・完全削除・読取拒否) まで通す。ローダーやファイル操作のスレッドは `ui()` と同じく `poll_*` を回して待つ |

決めごと:
- 実機依存 (Finder のゴミ箱、本物の brew、画面収録) は `#[ignore]` か偽物に差し替え、`cargo test` はオフラインで通す
- **操作できる部品は必ず `Response::widget_info` でラベルを持つ** (`nav_tab` / `button` / `icon_button` / `toggle_chip` / テーブルの列見出し・行 / シェルの窓ボタンと歯車)。ラベルは**描画と同じ大文字**にする。アイコンだけのボタンは `Icon::label()` (`REFRESH` など)。これが UI テストと支援技術の共通の入口
- 状態は egui の流儀で読む: `WidgetInfo::selected` は AccessKit の `toggled` に写るので、テストでは `node.accesskit_node().toggled() == Some(Toggled::True)`
- シェルは常時アニメして毎フレーム再描画を要求するので、kittest では `run()` (静止待ち) ではなく `run_steps(n)` + `with_step_dt` で決定的に進める
- ハーネスは生成時に最初のフレームを回すため、フォント登録 (`theme::install`) は最初のフレームで行い、そのフレームは何も描かない (`set_fonts` は次パスから有効)
- ダイアログはフェードインの最初のフレーム (opacity 0) では部品が無効 (egui は不可視の `Ui` を disable する) なので、E2E では `!accesskit_node().is_disabled()` になるまで待ってからクリックする
- 同じ文字列が複数の場所に出るとき (選択した行の名前がインスペクターにも出る等) は `get_by_role_and_label(Role::Button, ..)` で絞る
- E2E が見つけた実バグ: egui は Esc でフォーカスを先に外すので `has_focus()` では Esc を拾えない → `lost_focus()` も見る (フィルターの Esc クリアが動いていなかった)。brew の検索ビューでは `Cmd+F` を検索欄に向ける


## 配布 (.app / DMG、Apple Silicon)

```sh
cargo install cargo-bundle          # 初回のみ
./scripts/release.sh                # dist/FUIDE CAD.{app,dmg}
```

- `cargo bundle --format osx` で `.app`（`Info.plist`、`assets/icons/*.svg` から `.icns`）→ `codesign`（既定は ad-hoc）→ `hdiutil` で `/Applications` へのリンク入り DMG
- バンドル設定は各 `apps/*/Cargo.toml` の `[package.metadata.bundle]`（識別子 `fuide.file-manager` / `fuide.brew`、最小 macOS 13）。`icon` のパスは cargo-bundle を実行したディレクトリ基準なので、スクリプトはワークスペース root で実行する
- Spotlight から起動するには DMG を開いて `.app` を `/Applications` にドラッグ（インデックスに数十秒。急ぐなら `mdimport /Applications/FUI\ Brew.app`）
- **他の Mac に配る場合**: Developer ID で署名・公証していないので、受け取った側は初回だけ右クリック → 開く、または `xattr -d com.apple.quarantine "/Applications/FUIDE Brew.app"` が必要。Developer ID を取得したら `SIGN_IDENTITY="Developer ID Application: ..." ./scripts/release.sh` で署名し、`xcrun notarytool submit dist/*.dmg --wait` → `xcrun stapler staple` で公証
- FUIDE Brew は launchd 起動の最小 `PATH` でも動くよう `brew` を `/opt/homebrew/bin` → `/usr/local/bin` → `PATH` の順で探す

## ターミナルから開く (`open` 風)

```sh
./scripts/install-cli.sh            # /opt/homebrew/bin (書込可なら) or ~/.local/bin に fuide-cad を置く
fuide-cad                           # 新しいウィンドウ
fuide-cad part.cad.json             # ドキュメントを開く (相対パス可)
```

- `fuide-cad --mcp` はバンドル内のバイナリを `--mcp` で直接実行する (MCP の stdio ブリッジ。上の「AI エージェントから操作する」)
- `fuide-cad` は `open -na "FUIDE CAD" --args <絶対パス>` を呼ぶだけ。LaunchServices 経由なので Dock に出て、ターミナルを閉じても残る。`-n` で毎回新しいウィンドウ（プロセス）が開く
- `open` は起動先の cwd を `/` にするため、ラッパー側で `cd "$dir" && pwd -P` で絶対化してから渡している
- アプリは `/Applications` か `~/Applications` に入れておく（DMG からドラッグ）。`open` は LaunchServices のデータベースからバンドル名で探すので、パスは不要
