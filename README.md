# FUIDE — FUI Develop Environment

Sci-Fi / FUI (Futuristic UI) デザインのアプリを作るための開発環境。中核は egui (0.36) 向けの `fuide` クレート (テーマ・窓シェル・部品) で、その上にアプリとしてファイルマネージャー、Homebrew フロントエンド、オーディオ / 動画プレイヤー、アクティビティモニター (macOS デスクトップ) を載せています。今後はモバイルなどデスクトップ以外のアプリも同じ基盤で作る予定です。

```
crates/fuide/        FUI 部品ライブラリ `fuide` (egui のみ依存)
  theme.rs           パレット CYAN / AMBER / GREEN、フォント登録、ウィジェットスタイル
  shell.rs           フレームレス窓シェル (直角の枠、グロー、タイトル/ステータスバー、リサイズ)
  panel.rs           タイトルチップ付きパネル
  widgets.rs         ナビタブ、ボタン、セグメントバー、円弧ゲージ、ランプ、ログフィード、読み出し行
  fx.rs              走査線、走査帯
  geom.rs            多角形、グロー描画 (チャンファーはオプション)
  pathinput.rs       パス入力欄の `~` / 相対パス展開と Tab 補完 (ffm の GO TO、プレイヤーの OPEN)
crates/fuide-3d/     3D ビューポート `fuide-3d` (wgpu): Z-up オービットカメラ、ホログラム塗り + グローする線、egui ウィジェット
apps/file-manager/   FUIDE File Manager — Finder 風ファイルブラウザ (macOS)
apps/brew/           FUIDE Brew — Homebrew の GUI (brew info --json / search / streaming runner)
apps/player/         FUIDE Player — オーディオ / 動画プレイヤー (AVFoundation、ファイルと http(s) URL)
apps/activity-monitor/  FUIDE Activity Monitor — CPU / メモリ / エネルギー / ディスク / ネットワークのプロセス監視 (libproc / Mach / IOKit)
apps/cad/            FUIDE CAD — パラメトリック 3D CAD (Manifold のメッシュカーネル + truck、フィーチャー列 + 式、ねじ山、STL / JSON、MCP の CAD 専用ツール)
apps/git/            FUIDE Git — Git クライアント (読み書きとも `git` CLI、hunk 単位のステージ、fetch / push のストリーミング出力)
assets/fonts/        Orbitron (見出し) / Share Tech Mono (データ) — いずれも OFL
```

## FUIDE File Manager

```sh
cargo run -p fuide-file-manager            # $HOME から開始
cargo run -p fuide-file-manager -- /usr/bin
cargo test -- --ignored trash            # 実際にゴミ箱へ移動する統合テスト (捨てファイルを 1 つ ~/.Trash に残す)
```

- 左: よく使う場所 + `/Volumes` のボリューム、ストレージ使用率ゲージ
- 中央: 履歴ボタン、パンくず (クリックで移動、現在地をクリックするとパス入力ダイアログ)、フィルター、隠しファイル切替、ファイル一覧 (列ヘッダでソート)
- 右: 選択項目のインスペクター (種類 / サイズ / 日時 / パーミッション / パス)、OPEN / FINDER / PATH (パス文字列をクリップボードへ) / COPY / CUT / PASTE / RENAME / DELETE
- エラー (削除失敗 / ディレクトリ読取拒否 / OS で開けない) は大きな `ERROR` カード (`fuide::dialog::alert`) で通知。カードには操作名だけ、詳細はイベントログ。Enter / Space / Esc / ACKNOWLEDGE で閉じる。複数のエラーは順番に表示
- リネームと削除は FUI 風のモーダルダイアログ (`fuide::Dialog`、開閉とも 0.15 秒のフェード)。削除は既定でゴミ箱へ移動 (`NSFileManager.trashItem`、Finder から復元可)、ダイアログ内の PERMANENT で完全削除に切替 (枠が危険色になる)
- 下: イベントログ (長い行は折り返し、ドラッグ選択して Cmd+C でコピー可、溢れたらスクロール。**パネル上の帯をドラッグして高さを変えられ、タイトルチップ `EVENT LOG [-]` のクリックで開閉でき、どちらも設定ファイルに保存される**)、ステータスバー (`T+HH:MM:SS.s` 経過時間、24h 超で `T+1d …`、件数、FPS、FS リンク状態)

| 操作 | キー |
|---|---|
| 選択移動 / 開く / 親へ | ↑↓ / Enter / Backspace |
| 履歴 戻る / 進む | Cmd+[ / Cmd+]、マウスの戻る / 進むボタン |
| リネーム | Cmd+R (ダイアログ。Enter 確定 / Esc 取消) |
| ゴミ箱へ移動 / 完全削除 | Cmd+Backspace / Cmd+Option+Backspace (確認ダイアログ) |
| コピー / 切り取り / 貼り付け | Cmd+C / Cmd+X / Cmd+V (ファイル本体はアプリ内で保持し、OS クリップボードにはパス文字列を書く。egui は OS クリップボードが空だと Cmd+V の Paste イベントを出さないため。その後ほかのアプリで別のものをコピーすると Finder 同様ファイル側は無効になる。ログの文字選択中の Cmd+C はテキストコピーのまま。貼り付け先は今いるディレクトリ。同名があれば Finder 風に `name copy.ext` / `name copy 2.ext`。切り取った行は薄く表示され、貼り付けで移動してクリップボードは空になる。コピーは何度でも貼れる。ステータスバーに `CLIP n COPIED/CUT` ランプ) |
| フィルターにフォーカス / クリア | Cmd+F / Esc |
| パスを入力して移動 | Cmd+Shift+G またはパンくずの現在地をクリック (ダイアログ。現在のパスが入った状態で開き、`~` と相対パス・`..` を受け付け、Tab でディレクトリ名を補完 (候補は下に表示、複数なら共通部分まで)。ファイルのパスならその親へ移動してファイルを選択。Enter 移動 / Esc 取消) |
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
- 下: brew の標準出力・標準エラーをストリーミング表示 (`==>` = 緑、`Warning` = 注意色、`Error` = 危険色)。パネル上の帯をドラッグして高さを変えられ、タイトルチップ `BREW OUTPUT [-]` のクリックで開閉できる (どちらも保存される)
- 変更系 (update / upgrade / install / uninstall) は確認ダイアログ → 別スレッドで実行、完了後に在庫を再取得。成功は SUCCESS カード、失敗は ERROR カード。同時実行は 1 つ
- 読み取り系は `HOMEBREW_NO_AUTO_UPDATE=1` で呼ぶ (自動更新で数秒待たされないため)。全コマンドに `NONINTERACTIVE=1`、stdin は閉じるので sudo 待ちで固まらない
- キー: ↑↓ 選択、Enter ホームページ、Cmd+Backspace アンインストール、Cmd+R 再取得、Cmd+, 設定、Cmd+W 終了 (実行中の brew コマンドは止めない: 子プロセスはそのまま完走する)
- 撮影フック: `FUIDE_DEV_DIALOG=uninstall|upgrade|error|success`、`FUIDE_DEV_RUN="doctor"` (起動時に brew コマンドを流す)、`FUIDE_DEV_SEARCH=ripgrep`
- `FUIDE_BREW_BIN=/path/to/brew` で呼び出す `brew` を差し替えられる (テストは `fixtures/fake-brew.sh` を使う。`FUIDE_FAKE_BREW_LOG` にコールを記録)

## FUIDE Player

```sh
cargo run -p fuide-player                                   # 空のキューで起動
cargo run -p fuide-player -- ~/Movies/clip.mp4 song.m4a     # 引数をキューに入れて先頭を再生
cargo run -p fuide-player -- https://example.com/live.m3u8  # URL (HLS も) も同じ
FUIDE_DEV_MUTE=1 cargo run -p fuide-player -- apps/player/fixtures/clip.mp4   # 音を出さずに
```

デコーダは **macOS 標準の AVFoundation** (`objc2-av-foundation`)。ライブラリの同梱なし、ハードウェアデコード、対応フォーマットは「macOS が再生できるもの」= 普遍的なものだけに絞っている。

| 種別 | 対応 | 非対応 |
|---|---|---|
| 動画コンテナ | MP4 / M4V / MOV | MKV / WebM / AVI |
| 動画コーデック | H.264 / HEVC / ProRes、AV1 (M3 以降のハードウェア) | VP9 / VP8 |
| 音声 | MP3 / AAC (M4A) / ALAC / FLAC / WAV / AIFF | OGG Vorbis / Opus (.opus) |
| 入口 | ローカルファイル、`http(s)://` の URL (プログレッシブ MP4 / MP3、HLS `.m3u8`)、`file://` | それ以外のスキーム |

- 起動時は **シアターモード**: SCREEN とその下の TRANSPORT だけで、キュー / メディア情報 / ログは **Tab** で出し入れする。映像の矩形には何も描かない (走査線も除外)。ファイル名・埋め込みタイトル・状態 (PAUSED / BUFFERING / ACQUIRING / SIGNAL LOST)・コーデック・タイムコードは映像の上の 1 行 (HUD ストリップ) に出る。映像のクリックで再生 / 一時停止
- 左: QUEUE (キュー)。行クリックで選択、ダブルクリック / Enter で再生、Backspace で外す。OPEN FILE / OPEN URL / REMOVE / CLEAR。Finder や ffm からファイルをドロップするとキューに加わる (何も再生していなければ先頭が始まる)
- 中央上: SCREEN。HUD ストリップ (ファイル名 `::` タイトル、状態、コーデック・解像度・fps・音声、L/R レベルメーター、タイムコード `MM:SS.t / MM:SS.t`) の下に映像をレターボックスで等倍比表示。音声のみのときは曲名・アーティスト・アルバムと進捗リング、その下に **スペクトラム** (48 バンド対数周波数軸 40 Hz〜16 kHz、セグメント表示、ピークホールド)。音声は `MTAudioProcessingTap` で AVPlayer が実際に鳴らしている PCM を取り、UI スレッドで 2048 点 FFT する (HLS は AVFoundation がオーディオミックスを適用しないためスペクトラムなし)。映像が無い状態のプレート: `NO SIGNAL` (空) / `ACQUIRING SIGNAL` (読込中、走査帯) / `SIGNAL LOST` (失敗)
- 中央下: TRANSPORT。シークバー (ドラッグでスクラブ、ホバーで時刻、バッファ済み範囲を薄く表示。ライブ配信は走査帯)、PREVIOUS / PLAY / NEXT / STOP、時刻、右側に LOOP (OFF → ALL → ONE)、速度 (0.5 / 1 / 1.25 / 1.5 / 2X)、MUTE、音量バー
- 右: MEDIA。曲名、アーティスト / アルバム (埋め込みメタデータ)、ソース種別、コンテナ、長さ、映像 (コーデック / フレーム / fps / ビットレート)、音声 (コーデック / サンプルレート / チャンネル / ビットレート)、バッファ、場所 (パス or URL)。PLAY THIS / COPY (場所をクリップボードへ) / REMOVE
- 下: イベントログ (queue / play / end of track / 失敗)、ステータスバー (経過時間、キュー数、現在位置 / 長さ、FPS)、ランプ `STANDBY` / `ACQUIRING` / `PLAYING` / `PAUSED` / `BUFFERING` / `SIGNAL LOST`、`NET` (ネットワーク再生中)、`MUTED`
- 再生が終わるとループ設定に従って次へ (OFF: 次があれば次、無ければ停止。ALL: 末尾から先頭へ。ONE: 同じ曲をもう一度)。ロードに失敗した曲は `ERROR` カードで報告して外す (キューには残る)

| 操作 | キー |
|---|---|
| 再生 / 一時停止 | Space (何も読み込んでいなければ選択行、無ければ先頭を再生) |
| シーク | ← / → 5 秒、Shift 付きで 30 秒。シークバーのクリック / ドラッグ |
| 音量 / ミュート | ↑ / ↓ (5% 刻み) / M |
| 前後の曲 | Cmd+← / Cmd+→ (PREVIOUS は再生 3 秒以降なら曲頭へ戻る) |
| ループ / 速度 | L / S |
| 選択行を再生 / キューから外す | Enter / Backspace |
| ファイルを開く | Cmd+O または OPEN FILE: **macOS のファイルダイアログ** (`NSOpenPanel`、複数選択可)。選べるのは AVFoundation が再生できる種別だけ (`AVURLAsset.audiovisualContentTypes` でフィルタ)。選んだものをキューに入れて先頭を再生 |
| URL / パスを開く | Cmd+L または OPEN URL: アプリ内ダイアログ。http(s) URL のほか、ローカルパス (`~`・相対パス可、Tab でファイル・ディレクトリ名を補完) も受け付ける。Enter = PLAY、ADD TO QUEUE = 追加のみ。**MCP エージェントはこちらを使う** (macOS のダイアログの中はエージェントから見えない) |
| パネルの表示 / 非表示 | Tab (映像のクリックは再生 / 一時停止) |
| 全画面 | F、映像のダブルクリック、FULL ボタン。全画面中はシェル (タイトルバー・ステータスバー) も消え、Esc で戻る |
| キューの並べ替え | Cmd+↑ / Cmd+↓ で選択行を移動、または行をドラッグ (挿入位置に線が出る) |
| 字幕 | C または CC チップで OFF → 1 本目 → … → OFF。埋め込み字幕トラック (mov_text / HLS の WebVTT など、`AVMediaCharacteristicLegible` のメディア選択) を `AVPlayerItemLegibleOutput` で受け取り、映像の下の字幕バンドに描く (映像には重ねない) |
| チャプター | `[` / `]` で前後のチャプター (再生 3 秒以降の `[` は章頭へ)。シークバーに目盛り、HUD ストリップに `CH 2/3 タイトル`、MEDIA パネルにクリックで移動できる一覧。QuickTime のチャプタートラックを `chapterMetadataGroupsBestMatchingPreferredLanguages` で読む |
| パレット / 設定 / 終了 | Cmd+1..3 / Cmd+, / Cmd+W |

実装メモ:

- 再生は `AVPlayer` + `AVPlayerItem` (`AVURLAsset`)。UI スレッドが毎フレーム `Engine::poll` で状態 (status / currentTime / duration / loadedTimeRanges / timeControlStatus) を読む **ポーリング方式**で、KVO や通知は使わない。AVFoundation のオブジェクトは全部メインスレッドに置く (`Send` ではない)
- 映像は `AVPlayerItemVideoOutput` (32BGRA、IOSurface 裏付け) から `copyPixelBufferForItemTime` で取り出す。**既定は GPU 共有**: ピクセルバッファの IOSurface を `MTLDevice.newTextureWithDescriptor:iosurface:plane:` で Metal テクスチャにし、`wgpu::hal::metal::Device::texture_from_raw` → `Device::create_texture_from_hal` で wgpu に包み、`egui_wgpu::Renderer::register_native_texture` / `update_egui_texture_from_wgpu_texture` で egui のテクスチャ ID に載せる (コピーなし。4K でも CPU を使わない)。直近 3 フレームのバッファは GPU が読み終わるまで保持する。Metal 以外や IOSurface が取れない場合は BGRA → RGBA の CPU コピーに落ちる。どちらで動いているかは MEDIA パネルの `FRAMES` (GPU SHARED / CPU COPY) とログに出る
- トラック情報は item が `ReadyToPlay` になってから `AVPlayerItemTrack → AVAssetTrack → CMFormatDescription` (fourcc、寸法、fps、`AudioStreamBasicDescription`) を読む。メタデータ (title / artist / album) は `loadValuesAsynchronouslyForKeys` の完了フラグを見てから `commonMetadata` を読む (ネットワーク上のアセットで同期アクセスすると UI が止まるため)
- 終端検出は `actionAtItemEnd = Pause` にして「再生中だったのに止まり、位置が duration に達した」で判定。ライブ配信は duration が不定なので `LIVE` 表示
- **AVFoundation は状態更新をメインスレッドの run loop (main dispatch queue) 経由で届ける**。cargo test のワーカースレッドではいつまでも `Loading` のままなので、実エンジンのテストは `harness = false` の統合テスト (`apps/player/tests/engine.rs`) がメインスレッドで `CFRunLoopRunInMode` を回しながら行う。アプリ側の状態遷移と E2E は `Backend` トレイトの偽実装 (`player::fake::FakeBackend`) で回す
- 撮影フック: `FUIDE_DEV_DIALOG=open|error`、`FUIDE_DEV_MUTE=1`。テスト用メディアは `apps/player/fixtures/` (`clip.mp4` = ffmpeg の testsrc 2 秒 H.264 + AAC、`tone.m4a` = 3 秒のサイン波 AAC、`chapters.mp4` = 6 秒で 3 チャプター + mov_text 字幕、いずれも title / artist 付き)

## FUIDE Activity Monitor

```sh
cargo run -p fuide-activity-monitor
cargo run -p fuide-activity-monitor --example probe    # UI 無しで 2 回サンプリングして数値を出す (Activity Monitor と突き合わせる用)
```

macOS の「アクティビティモニタ」と同じ 5 タブ構成。**上: CPU / MEMORY / ENERGY / DISK / NETWORK** のタブ (Cmd+1..5)、**中央: プロセス一覧** (列はタブごとに変わる。列見出しでソート、Cmd+F でフィルター (名前 / ユーザー / PID)、MY PROCESSES で自分のプロセスだけ)、**右: 選択中のプロセスの詳細** (何も選んでいなければこの Mac の概要)、**下: タブごとの機械全体の要約** (時系列グラフ + 読み出し) とイベントログ。更新間隔はツールバーの `1 S / 2 S / 5 S`。サンプリングは別スレッドで行い、UI は他のアプリと同じくアイドル 20 fps。

| タブ | プロセス列 | 要約 |
|---|---|---|
| CPU | % CPU、CPU TIME、THREADS、WAKEUPS (アイドルウェイクアップ /s)、STATE | 合計 / システムの時系列、コア別セグメントバー (P / E クラスタ別)、user / system / idle、負荷平均、GPU 使用率 |
| MEMORY | MEMORY (phys footprint = アクティビティモニタの「メモリ」列)、RESIDENT、THREADS | 使用率の時系列、円弧ゲージ、メモリプレッシャー、物理 / 使用中 / App / ワイヤード / 圧縮 / キャッシュ / スワップ |
| ENERGY | ENERGY (推定)、% CPU、WAKEUPS、NO SLEEP (スリープを妨げている) | 合計の時系列、バッテリー残量 / 電源 / 残り時間 (バッテリーの無い Mac は NONE)、スリープを妨げているプロセス |
| DISK | READ/S、WRITE/S、TOTAL READ、TOTAL WRITE | 読み書きの時系列、IO/s、byte/s、起動以来の合計 |
| NETWORK | IN/S、OUT/S、TOTAL IN、TOTAL OUT | 送受信の時系列、パケット/s、byte/s、合計 |

| 操作 | キー |
|---|---|
| タブ | Cmd+1..5、または上のタブをクリック |
| フィルター | Cmd+F → 入力、Esc でクリア |
| 選択 | 行クリック、↑ / ↓、Esc で解除 |
| 終了 / 強制終了 | QUIT (SIGTERM) / FORCE QUIT (SIGKILL) ボタン、Cmd+Alt+Q / Cmd+Alt+Shift+Q。どちらも確認ダイアログ (枠色 = 注意 / 危険)。エージェントは人間留保 |
| 今すぐサンプリング / 設定 | Cmd+R / Cmd+, |

データの出どころ (root 無しで動かすための決めごと):

- **自分のプロセス**は `libproc` (`proc_pidinfo` / `proc_pid_rusage`) で区間計測の CPU %、phys footprint、スレッド数、アイドルウェイクアップ、ディスク読み書き byte、コマンドライン (`KERN_PROCARGS2`) を取る。CPU 時間は mach tick 単位なので `mach_timebase_info` で秒に直す (Apple Silicon では 125/3 ns)
- **他ユーザーのプロセス**には `libproc` が `EPERM` を返す (アクティビティモニタ・`top`・`ps` は setuid root)。そこで一覧のベースは `ps -axo ...` (setuid、約 40 ms) から取り、libproc が答えた行だけ上書きする。ps 由来の値は **`~` 付き** (CPU % はカーネルの減衰平均、メモリは RSS) で、取れない列は `--`。QUIT も自分のプロセス以外は拒否されるので、ダイアログに先に書いてある
- **プロセスごとのネットワーク**には公開 API が無い。`nettop -n -P -L 1 -x -J bytes_in,bytes_out` (user 権限で動く、約 10 ms。`-n` を忘れると名前解決で数秒かかる) を毎サンプル呼んで累積 byte を取り、差分でレートにする
- 機械全体: CPU は `host_processor_info` (コアごとの tick、E コアが先の番号)、メモリは `host_statistics64` (App = internal − purgeable、キャッシュ = external + purgeable、使用中 = App + ワイヤード + 圧縮)、スワップ `vm.swapusage`、プレッシャー `kern.memorystatus_vm_pressure_level`、ディスクは IOKit `IOBlockStorageDriver` の `Statistics`、GPU は `IOAccelerator` の `PerformanceStatistics`、ネットワークは `sysctl NET_RT_IFLIST2` (`if_data64`、ループバック除外)、バッテリーは `IOPSCopyPowerSourcesInfo`、スリープ阻止は `IOPMCopyAssertionsByProcess` (`AssertType` キー)
- **ENERGY は推定値**: Apple の「エネルギー影響」の式は非公開 (`powermetrics` は root 必須) なので、CPU % + ウェイクアップ + ディスク / ネット量の加重 (`sys::energy_estimate`) を出し、パネルに `ESTIMATE :: NOT APPLE'S SCALE` と明記している。順位付けには使える
- テストは `sys::Source` トレイトの偽実装 (`sys::fake::FakeSource`、10 プロセスの固定マシン) で回す。`sys::mac` の単体テストだけ実機を読む (自分のプロセスが Full、pid 1 が Limited になること)

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

## FUIDE Git

```sh
cargo run -p fuide-git                 # 最近開いたリポジトリ (無ければカレントディレクトリ)
cargo run -p fuide-git -- ~/src/repo   # リポジトリを指定して開く
```

Git クライアント ([#4](https://github.com/kobago/fuide/issues/4))。**libgit2 / gitoxide は使わず、読み書きともに `git` CLI だけ**を呼ぶ (`apps/git/src/git.rs`)。読み取りは `status --porcelain=v2 -z` / `log` / `for-each-ref` / `diff` / `show` を別スレッドで実行して 1 メッセージで返す。変更系 (`add` / `restore` / `commit` / `switch` / `fetch` / `pull` / `push` / `apply`) はすべて brew と同じストリーミング runner を通り、出力が 1 行ずつログに流れ、完了後にリポジトリを読み直す。同時実行は 1 つ。fetch / push は git 自身の credential helper に任せる (`GIT_TERMINAL_PROMPT=0` なので対話は起きず、失敗は ERROR カード)。

- **左上: REPOSITORY** — 名前、パス、ブランチ、upstream、ahead / behind、OPEN (パス入力ダイアログ、Tab 補完) / FETCH、最近開いたリポジトリ (`~/Library/Application Support/FUIDE/git-recent.conf`)。Finder や ffm からディレクトリをドロップしても開く
- **左下: BRANCHES** — NEW BRANCH (`switch -c`)、LOCAL / REMOTES / TAGS の一覧 (現在のブランチが点灯、右に upstream)。**ダブルクリックで切替** (`switch`。リモートは同名のローカルを作って追跡、タグは detach)
- **中央: CHANGES ビュー** (Cmd+1) — UNSTAGED / STAGED の 2 表 (ST / PATH)。行クリックで下に diff、**ダブルクリックか Space でステージ / アンステージ**、STAGE ALL (`add -A`、Cmd+A) / UNSTAGE ALL (`reset`) / DISCARD (`restore` または untracked は `clean -f`、危険色の確認ダイアログでエージェントは人間留保)。下の DIFF は行番号 (旧 / 新)、追加 = 緑、削除 = 危険色、hunk 行に **STAGE HUNK / UNSTAGE HUNK** (`git apply --cached [-R]` にその hunk だけの patch を流す)
- **中央: HISTORY ビュー** (Cmd+2) — `log --all` の直近 500 件 (HASH / SUBJECT / AUTHOR / WHEN、装飾付きは accent)。行を選ぶと右にコミット詳細、その変更ファイルをクリックすると下に diff (`show <hash> -- path`)
- **右: COMMIT** (CHANGES ビュー) — メッセージ欄と COMMIT (staged があり、メッセージが空でないとき。`commit -F -` で stdin から渡す。Cmd+Enter は欄にフォーカスがあっても効く)。HISTORY ビューでは選択コミットの件名 / 本文 / hash / author / date / parents / refs、COPY HASH、ファイル一覧
- **ツールバー**: 再読込 (Cmd+R)、ビュー切替、PULL (`--ff-only`、behind 数付き) / PUSH (ahead 数付き。upstream が無ければ `-u origin <branch>`)
- **下: GIT OUTPUT** — コマンドの標準出力 / 標準エラー (`error` / `fatal` = 危険色、`warning` / `hint` = 注意色)。帯のドラッグで高さ変更、チップのクリックで開閉

| 操作 | キー |
|---|---|
| 選択移動 | ↑↓ (フォーカス中の表: UNSTAGED / STAGED / HISTORY) |
| ステージ / アンステージ | Space または Enter (選択行)、ダブルクリック |
| 全部ステージ | Cmd+A |
| コミット | Cmd+Enter |
| リポジトリを開く / 再読込 | Cmd+O / Cmd+R |
| ビュー | Cmd+1 (CHANGES) / Cmd+2 (HISTORY) |
| 設定 / 終了 | Cmd+, / Cmd+W |

まだ無いもの: コミットグラフの線、reset / force push / branch -D、マージ競合の解決、500 件より古いログの段階読み込み、MCP の Git 専用ツール。

撮影フック: `FUIDE_DEV_DIALOG=diff|history|discard|open|branch|error|success`。テスト (`cargo test -p fuide-git`) は一時ディレクトリに `git init` した実リポジトリで回る (ネット不要。`GIT_CONFIG_GLOBAL=/dev/null` で署名などの個人設定を外す)。

## 設定ウィンドウ (テーマ)

各アプリとも `Cmd+,` かタイトルバーの歯車で設定ウィンドウが開く。パレット (CYAN / AMBER / GREEN)、角 (SQUARE / CHAMFER)、密度 (NORMAL / COMPACT)、窓の透過 (WINDOW: TRANSLUCENT / OPAQUE。OPAQUE は本体の地色 `bg_deep` を不透明にしてデスクトップが透けないようにする。窓自体は透過のままなので、枠の外側のグローや面取りした角はこれまで通り抜ける)、AGENT (MCP サーバーの OFF / ON、確認ダイアログを HUMAN / AGENT のどちらが押すか。下の「AI エージェントから操作する」) を選ぶと即座に本体へ反映され、ファイルに保存される。閉じるのは × / Esc / Cmd+W。

- 設定ウィンドウは egui の **子 viewport** (別のネイティブウィンドウ、`show_viewport_deferred`) で、本体と同じ `fuide::Shell` を `tool_window()` (閉じるボタンのみ・リサイズなし・アイドルアニメ無し = 入力があったときだけ再描画) で描いている。フォントや Visuals は `egui::Context` 全体で共有なので、子ウィンドウで変えた瞬間に本体も変わる
- 子 viewport は eframe 0.36 では撮影できない (immediate は `Screenshot` コマンドを捨てる。deferred は macOS でイベントループが約 1 秒止まったあと再描画が来なくなる)。撮影は `FUIDE_DEV_EMBED=1` で本体に埋め込んで行う (上の「開発用スクリーンショット」)
- 保存先は macOS では `~/Library/Application Support/FUIDE/<app>.conf` (`file-manager.conf` / `brew.conf` / `player.conf`)、他 OS では `$XDG_CONFIG_HOME/fuide/` か `~/.config/fuide/`。`FUIDE_CONFIG_DIR` で置き換え可。中身は `palette=amber` のような `key=value` 行 (`palette` / `chamfer` / `compact` / `transparent` / `agent` / `agent_confirm`、ログパネルをドラッグすると `log_height`、ログパネルを開閉すると `log_open`) で、知らないキーは無視、足りないキーは既定値
- 自作アプリで使うには `fuide::Settings` と `fuide::SettingsWindow` (下の「クレートの使い方」参照)

## AI エージェントから操作する (MCP)

各アプリは **MCP サーバー** を内蔵している。設定ウィンドウ (`Cmd+,`) の AGENT パネルで `ON` にすると Unix ソケットで待ち受け、Claude Code などの MCP クライアントが画面を読み・クリックし・文字を打てる。人が見ている前で AI が FUI を操作するための機能なので、操作は画面に見える形で行われる: エージェント用の照準カーソルが目標までなめらかに移動し、押した部品が光り、直前の操作 (`CLICK ▸ OUTDATED`) がカーソル脇に出る。ステータスバーには `AGENT` ランプが点く (操作中は点滅)。

```sh
# Claude Code に登録 (ラッパーを入れていれば `ffm --mcp` / `fuide-brew --mcp` でも良い)
claude mcp add fuide-brew -- "/Applications/FUIDE Brew.app/Contents/MacOS/fuide-brew" --mcp
claude mcp add ffm        -- "/Applications/FUIDE File Manager.app/Contents/MacOS/fuide-file-manager" --mcp
claude mcp add fuide-player -- "/Applications/FUIDE Player.app/Contents/MacOS/fuide-player" --mcp
claude mcp add fuide-activity-monitor -- "/Applications/FUIDE Activity Monitor.app/Contents/MacOS/fuide-activity-monitor" --mcp
claude mcp add fuide-cad -- "/Applications/FUIDE CAD.app/Contents/MacOS/fuide-cad" --mcp
# 開発中は cargo のバイナリでも同じ
claude mcp add fuide-brew -- target/debug/fuide-brew --mcp
```

| ツール | 内容 |
|---|---|
| `observe` | アプリの状態要約 (表示中のビュー・選択・実行中のコマンド・ダイアログ・ログ末尾) と、画面上の操作できる部品の一覧 `[role] LABEL (state) @x,y`。最初に呼び、各操作のあとも返ってくる |
| `click {label, nth?}` | ラベルの部品へカーソルを動かしてクリック。ラベルは `observe` に出る文字列そのまま (大文字)。完全一致 → 大文字小文字無視 → 部分一致の順で探す |
| `type {text, label?, submit?}` | 入力欄に 1 文字ずつ打つ。`label` を付けるとその欄にフォーカスしてから。`submit` で最後に Enter |
| `key {key, repeat?}` | `enter` / `escape` / `down` / `cmd+3` / `cmd+f` / `cmd+backspace` など |
| `wait {ms}` | brew の実行やディレクトリ読込を待ってから観測を返す |
| `screenshot {scale?, path?}` | 窓を PNG で返す (画面収録権限は不要。`FUIDE_SCREENSHOT` と同じ自己撮影)。`path` を付けると保存もする |
| アプリ固有のツール | アプリが `Agent::set_tools` で足したもの (CAD の `add_feature` / `measure` など)。`tools/list` に並び、アプリ側で処理されて、結果の文の後に観測が付く。部品を押すわけではないので、代わりに**ツールが触った部品 (追加した行、書き換えた入力欄) へカーソルが飛んで光り**、脇に `TOOL ▸ ADD_FEATURE` と出る (`Agent::finish_tool` の `focus`)。`--mcp` ブリッジも同じ一覧を答える (`bridge::run_with_tools`) |

仕組みと決めごと:
- **クリックの注入**: egui の `Event::AccessKitActionRequest(Click)` を対象ウィジェットの id に向けて入れる。egui はこれを本物のクリックとして扱う (`Response::clicked()` が真になる) ので、座標を当てる必要がなく、部品が動いても壊れない。キーと文字は `Event::Key` / `Event::Text` で、`Cmd` などの修飾キーはそのフレームの `InputState::modifiers` に載せる
- **部品の一覧**: kit の部品は `Response::widget_info` の代わりに `fuide::agent::describe` を呼び、アクセシビリティ木への登録と同時にエージェント用の一覧にも載る (ラベル・種類・状態・矩形)。`egui::TextEdit` のように自前で木に載る部品は `fuide::agent::note` で一覧だけに足す。アプリ側は `agent_state()` で部品だけでは分からない状態を文章にして渡す
- **モーダル中は、ダイアログの部品しか操作できない** (前景レイヤーに部品があればそれだけを列挙する)。AccessKit 経由のクリックはモーダルの背後にも届いてしまうため
- **確認ダイアログの人間留保**: 設定の CONFIRM DIALOGS が `HUMAN` (既定) の間、brew の `UPGRADE` / `UNINSTALL` / `INSTALL`、ファイルマネージャーの `MOVE TO TRASH` / `DELETE PERMANENTLY` のボタンと Enter はエージェントに拒否され、観測に `(human only)` と出る。`CANCEL` は押せる。`AGENT` にすると自分で確定できる。リネームは可逆なので留保しない
- **通信**: アプリが `~/Library/Application Support/FUIDE/<app>.sock` (パスが長すぎるときは `$TMPDIR/fuide-<app>.sock`) で MCP (JSON-RPC 2.0、改行区切り) を話す。`<app> --mcp` は同じバイナリの stdio ブリッジで、`initialize` / `tools/list` は自分で答え、`tools/call` だけをソケットへ転送する。だから **Claude Code はアプリより先に起動していてよい**: 最初の呼び出しでアプリが無ければ `open -a` で起動して 12 秒待ち、AGENT が OFF なら「設定で ON にして」というエラーを返す
- 設定ウィンドウ (子 viewport) 自体はエージェントから操作できない (人間の操作面)。`FUIDE_DEV_EMBED=1` で本体に埋め込んだときは操作できる
- 依存は増やしていない: JSON は `serde_json`、PNG は macOS の `sips` で圧縮 (無ければ非圧縮 PNG を自前で書く)、base64 も自前

```sh
# 手で試す (nc は改行区切りの JSON-RPC をそのまま流せる)
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"observe","arguments":{}}}' \
  | nc -U ~/Library/Application\ Support/FUIDE/brew.sock
```

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
| 単体 (アプリ) | `apps/*/src/*.rs` | `fs.rs` / `brew.rs` の純関数。CAD は `mesh.rs` (Manifold: 穴あき板の体積、共平面の UNION / CUT / INTERSECT が厳密に一致、任意軸の円柱と回転、六角ボルト + 本物のねじ山の UNION、STL)、`kernel.rs` (truck: 同じ検証 + 公差のはしご、共平面のナッジ、panic の捕捉)、`expr.rs` (式)、`doc.rs` (JSON 往復、削除の拒否、フィールド)、`eval.rs` (穴あき板、抑制、エラーの伝播、変換、3 軸のねじ) |
| 状態機械 (アプリ) | `apps/*/src/app/tests.rs` | `Explorer::with_context(ctx, dir, settings)` / `BrewApp::with_context(ctx, settings)` で `CreationContext` 無しにアプリを作り、`Action` を適用して状態・ログ・ダイアログを検証。ファイルマネージャーは一時ディレクトリで実ファイル操作 (一覧・ソート・フィルター・履歴・リネーム・完全削除・読取拒否) まで通す。ローダーやファイル操作のスレッドは `ui()` と同じく `poll_*` を回して待つ |
| エージェント (fuide) | `crates/fuide/tests/agent.rs` | kittest 上で `Agent::submit` に `observe` / `click` / `type` / `key` / `screenshot` を流し、注入したクリックが kit の部品に届くこと、無効・人間留保・不明なラベルが拒否されること、PNG が返ることを検証 (ソケット無し) |
| E2E (アプリ) | `apps/*/src/app/e2e.rs` | `egui_kittest` の `Harness::new_eframe` で本物の `Explorer` / `BrewApp` を起動し、アクセシビリティ木からラベルでクリック・キー入力・文字入力して状態を検証。brew はエージェント経由 (ビュー切替・行選択・Cmd+1・フィルター入力、確認ダイアログの人間留保と `agent_confirm` での確定) も通す。ファイルマネージャー: 行クリック → Enter で移動 / Backspace / Cmd+[ ] / 矢印、Cmd+F → 入力 → Esc、歯車 → パレット・角の変更が保存される。brew (偽 brew): ビュー切替 (タブ / Cmd+数字)、UPGRADE ALL → 確認 → 出力ストリーム → SUCCESS カード → ACKNOWLEDGE、検索ビューで Cmd+F → 入力 → Enter、Cmd+, → パレット保存。設定ウィンドウは kittest では埋め込み `egui::Window` になる。プレイヤー (偽バックエンド): Cmd+L → URL 入力 → Enter で再生開始、Space / PLAY / 矢印 / M / S / STOP、行クリック → Enter、PREVIOUS / NEXT、Backspace で外す、CLEAR。アクティビティモニター (偽ソース): タブ (クリック / Cmd+5) で列が変わる、行を名前でクリック → QUIT が有効に、↑ ↓ / Esc、Cmd+F → 入力 → Esc、QUIT → CANCEL / FORCE QUIT → Enter でプロセスが一覧から消える、root のプロセスは ERROR カード、5 タブ + ダイアログのスナップショット。CAD (実カーネル + wgpu ビューポート): BOX → CYLINDER → 欄に打ち替え → 行 → CUT → 行で穴あき板、`SIZE.Z` の打ち替えで体積が倍になり Cmd+Z で戻る、WIRE / TOP / 数字キー、Cmd+S のダイアログ、エージェントの専用ツール (`set_param` → `add_feature` ×3 → `measure`、拒否される `set_field` / `remove_feature`、`export` の上書き拒否、`document` / `open` / `view`)、サンプルのスナップショット (時計固定・ログ差し替え) |
| 統合 (実エンジン) | `apps/player/tests/engine.rs` | `harness = false` でメインスレッドを確保し、実 AVFoundation で WAV (PCM、再生完了・再開) と MP4 (H.264 / AAC、メタデータ、フレームのテクスチャ化、速度・音量) と存在しないファイルの失敗を確認。`CFRunLoopRunInMode` でメインの run loop を回しながらポーリングする |
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
./scripts/release.sh                # dist/FUIDE File Manager.{app,dmg}, dist/FUIDE Brew.{app,dmg}, Player, Activity Monitor, CAD
./scripts/release.sh fuide-brew       # 1 本だけ
```

- `cargo bundle --format osx` で `.app`（`Info.plist`、`assets/icons/*.svg` から `.icns`）→ `codesign`（既定は ad-hoc）→ `hdiutil` で `/Applications` へのリンク入り DMG
- バンドル設定は各 `apps/*/Cargo.toml` の `[package.metadata.bundle]`（識別子 `fuide.file-manager` / `fuide.brew`、最小 macOS 13）。`icon` のパスは cargo-bundle を実行したディレクトリ基準なので、スクリプトはワークスペース root で実行する
- Spotlight から起動するには DMG を開いて `.app` を `/Applications` にドラッグ（インデックスに数十秒。急ぐなら `mdimport /Applications/FUI\ Brew.app`）
- **他の Mac に配る場合**: Developer ID で署名・公証していないので、受け取った側は初回だけ右クリック → 開く、または `xattr -d com.apple.quarantine "/Applications/FUIDE Brew.app"` が必要。Developer ID を取得したら `SIGN_IDENTITY="Developer ID Application: ..." ./scripts/release.sh` で署名し、`xcrun notarytool submit dist/*.dmg --wait` → `xcrun stapler staple` で公証
- FUIDE Brew は launchd 起動の最小 `PATH` でも動くよう `brew` を `/opt/homebrew/bin` → `/usr/local/bin` → `PATH` の順で探す

## ターミナルから開く (`open` 風)

```sh
./scripts/install-cli.sh            # /opt/homebrew/bin (書込可なら) or ~/.local/bin に ffm / fuide-brew / fuide-player / fuide-activity-monitor / fuide-cad を置く
ffm                                 # カレントディレクトリを開く
ffm ~/Downloads                     # 指定ディレクトリを開く (相対パス可)
fuide-brew
```

- `ffm --mcp` / `fuide-brew --mcp` はバンドル内のバイナリを `--mcp` で直接実行する (MCP の stdio ブリッジ。上の「AI エージェントから操作する」)
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

`NativeOptions.viewport` は `with_decorations(false).with_transparent(true)`、`App::clear_color` は `[0.0; 4]` にする (設定の WINDOW = OPAQUE でも窓は透過のまま。`Settings::apply` がパレットの `bg_deep` を `Palette::opaque` で不透明にして本体を塗りつぶす)。`Shell` は Cmd+W で自分のウィンドウに `ViewportCommand::Close` を送る (本体なら終了、`tool_window()` は自前で閉じる)。

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
