<p align="right"><a href="README.md">English</a> | <b>日本語</b></p>

<p align="center">
  <img src="asset/ibisOffice_svg_logo_for_light.svg" alt="ibisOffice" width="480">
</p>

<p align="center">
  <b>Rust と GPU で作った、Microsoft Office と同じ見た目で開ける次世代のオフィス スイート</b><br>
  Word（.docx）・Excel（.xlsx）・PowerPoint（.pptx）を、速く、正確に、どこでも。
</p>

---

<p align="center">
  <img src="asset/screenshots.png" alt="ibisOffice で開いた Word・Excel・PowerPoint の文書" width="100%">
</p>

## ibisOffice とは

ibisOffice は、Word・Excel・PowerPoint の文書を、表示し、編集し、保存するためのオフィス スイートです。すべてを Rust で書き、描画は GPU で行います。macOS・Windows のデスクトップでも、Web ブラウザでも、同じ 1 つのコードで動きます。

目指しているのは、「Microsoft Office で作った文書を開いても、見た目が崩れない」ことです。行の折り返し、ページの区切り、表の罫線、グラフの目盛りまで、Office と同じ結果になるように作っています。

## すぐに始める

- **デスクトップアプリ版をダウンロード**：[Releases](https://github.com/ibisjp/ibis-office/releases/latest) から
  - macOS（11 以降）：`ibisOffice-<版>-macOS.zip` をダウンロードし、展開した `ibisOffice.app` をアプリケーション フォルダに移してください。
  - Windows（64 ビット）：`ibisOffice-<版>-Windows.zip` をダウンロードし、展開した `ibisOffice` フォルダの `ibisOffice.exe` を起動してください。
  - Linux（x86_64）：Linux 版はこのフォーク [lunagoat/ibis-office](https://github.com/lunagoat/ibis-office/releases/latest) が公開しています。`curl -fsSL https://raw.githubusercontent.com/lunagoat/ibis-office/HEAD/scripts/install-linux.sh | sh` を実行すると、いまのユーザー向けに導入されます。詳しくは [Linux](#linux) を見てください。
- **Web 版を試す**：インストールせずに、ブラウザで [playground](https://ibisjp.github.io/ibis-office/) を開いてください（WebGPU に対応したブラウザ）。見本の文書（Word・Excel・PowerPoint）を開いて、すぐに試せます。
- **動画で見る**：ibisOffice の紹介動画（YouTube）。

  [![ibisOffice の紹介動画](asset/video-ja.jpg)](https://www.youtube.com/watch?v=0yjHdK_Cb_8)

## ゴール

ibisOffice は、世界でトップクラスのオープンソースのオフィス スイートになることを目指しています。

- **速さと堅牢さ**：Rust によるメモリ安全なコードで、大きな文書もすばやく開いて、落ちない。
- **Microsoft Office と同じ見た目**：文字の位置、改行、改ページ、表、グラフ、図形を、Office と同じに描く。
- **Microsoft Office と同じファイル**：開いて保存しても、Office で開き直したときに中身も見た目も失われない。
- **OS を選ばない**：デスクトップアプリ版は、macOS・Windows・Linux で、同じ操作と同じ見た目。
- **ブラウザで、ネイティブと同じ速さ**：WebAssembly と WebGPU で、インストールせずに使える。
- **共同編集**：サーバー版で、複数の人が同じ文書を同時に編集できる。

## 特長

### 3 つの形式を 1 つのアプリで

- **文書（.docx）**：段落とスタイル、箇条書きと段落番号、表、段組み、縦書き、ルビ、脚注、ヘッダーとフッター、フィールド（ページ番号・目次）、図形・画像・グラフ、ページ設定。
- **表計算（.xlsx）**：数式と関数の計算、条件付き書式、オートフィルター、並べ替え、テーブル、ピボットテーブル、グラフ、スパークライン、入力規則、シートの保護、セルの中の部分書式、ゴールシーク。
- **プレゼンテーション（.pptx）**：図形・テキスト・表・グラフ・SmartArt の表示と編集、アニメーション、画面切り替え、スライドショーと発表者ビュー（2 画面対応）、動画・音声、コメント。
- **マクロ付きのファイル**（.docm・.xlsm・.pptm）も開いて保存できます。マクロは実行せず、そのまま残します。

### Office と同じ見た目を、測って確かめる

見た目の互換は、感覚ではなく測定で確かめています。機能ごとに試験用の文書を作り、本物の Word・Excel・PowerPoint が書き出した PDF と ibisOffice の描画を、画素の単位で比べています。保存したファイルは、Office で修復なしに開けることを確かめています。

### GPU で描く、速い画面

画面は GPU で描きます（macOS は Metal、Windows は DirectX、Linux は Vulkan、ブラウザは WebGPU）。スクロール、拡大・縮小、シートの切り替えが滑らかです。UI は、高速なエディター Zed のために作られた UI フレームワーク GPUI で作っています。

### デスクトップでもブラウザでも

同じ Rust のコードが、ネイティブのアプリとしても、WebAssembly としてブラウザの中でも動きます。

## 強み

- **ゼロから Rust で書いた、新しい実装**：既存のオフィス スイートの古いコードを引き継いでいません。メモリ安全で、並列に強く、速い。
- **描画の互換を、仕組みで保証する**：7 万を超える試験で、Office の出力と画素の単位で比べ続けています。
- **編集はすべて「コマンド」**：すべての変更は、ID と確定した値だけを持つコマンドを通ります。これは、元に戻す・やり直しの単位であり、共同編集でサーバーとやり取りする単位にもなります。
- **規格に忠実**：Office Open XML の国際標準（ECMA-376 / ISO/IEC 29500 / JIS X 4191）に沿って読み書きします。図形の輪郭などの組み込みの定義は、Ecma が配布する ECMA-376 の公式のデータをそのまま使っています。
- **商用でも安心して使える部品だけ**：依存するライブラリは、Apache-2.0・MIT・BSD などの商用で使えるライセンスのものだけです。コピーレフトのライブラリは使っていません（`cargo deny` で毎回確かめています）。

## 動機

私たちは、Google Drive のようなファイルの保存・共有のサービス ibisStorage を提供しています。そこでは、Office のファイルのオンラインでのプレビューと編集を、速く、正確に、サーバーに負荷をかけずに行えるようにする必要があります。これが ibisOffice を作る動機です。

## 速さ

小さいほど速く、かっこ内は ibisOffice の何倍かかったかを表します。

### ファイルを開いて最初の画面が出るまで

アプリを起動しておき、ファイルを開かせてから、1 ページ目（pptx は 1 枚目のスライド、xlsx は 1 シート目の最初の画面）が完全に表示されるまでの時間です。

<p align="center">
  <img src="asset/benchmark-open-ja.svg" alt="ファイル表示速度ベンチマーク: ibisOffice vs Microsoft Office, LibreOffice, ONLYOFFICE" width="100%">
</p>

| ファイル | ibisOffice | Microsoft Office | LibreOffice | ONLYOFFICE |
|---|---:|---:|---:|---:|
| プレゼンテーション（.pptx）<br>31 スライド、35.9MB | **0.59 秒** | 2.16 秒（3.7 倍） | 4.40 秒（7.5 倍） | 4.75 秒（8.1 倍） |
| 文書（.docx）<br>英文 140〜154 ページ、0.12MB | **0.45 秒** | 1.11 秒（2.5 倍） | 1.70 秒（3.8 倍） | 3.15 秒（7.0 倍） |
| 表計算（.xlsx）<br>22 シート、グラフ 29 個、数式 58.9 万個、13.7MB | **0.52 秒** | 14.07 秒（27.0 倍） | 6.82 秒（13.1 倍） | 9.36 秒（17.9 倍） |

アプリのウィンドウを録画し（最大 60 フレーム/秒）、ファイルを開かせた時点から画面の変化が止まるまでを、動画から求めました。終わりのフレームは目で確かめています。

### PDF への変換

コマンドラインで PDF に変換し、コマンドを始めてから PDF を書き終えるまでの時間です。

<p align="center">
  <img src="asset/benchmark-pdf-ja.svg" alt="PDF 変換速度ベンチマーク: ibisOffice vs Microsoft Office, LibreOffice, ONLYOFFICE" width="100%">
</p>

| ファイル | ibisOffice | Microsoft Office | LibreOffice | ONLYOFFICE |
|---|---:|---:|---:|---:|
| プレゼンテーション（.pptx）<br>31 スライド、35.9MB | **1.42 秒** | 8.40 秒（5.9 倍） | 32.26 秒（22.8 倍） | 5.87 秒（4.1 倍） |
| 文書（.docx）<br>英文 140〜154 ページ、0.12MB | **0.69 秒** | 12.49 秒（18.1 倍） | 3.45 秒（5.0 倍） | 4.19 秒（6.1 倍） |
| 表計算（.xlsx）<br>22 シート、グラフ 29 個、数式 58.9 万個、13.7MB | **7.26 秒** | 42.94 秒（5.9 倍） | 19.46 秒（2.7 倍） | 24.89 秒（3.4 倍） |

変換に使ったもの：ibisOffice は `ibisOffice --export pdf`、Microsoft Office は AppleScript で開いて PDF で保存、LibreOffice は `soffice --headless --convert-to pdf`、ONLYOFFICE はアプリに入っている変換プログラム x2t。

### 計測の条件

- 計測環境：Apple M2 Pro、メモリ 16GB、macOS 26.5.1。製品の版：ibisOffice 0.1.0（開発版）、Microsoft Office 16.113.2、LibreOffice 26.8.0.3、ONLYOFFICE 9.4.0。計測日：2026 年 9 月 29〜30 日。
- 各 1 回の計測です。どの計測も、そのアプリのプロセスがすべて終了した状態から始めました。
- 使ったファイルは、架空のデータと、メタ情報を取り除いた公開の資料から作ったもので、`test/benchmark/` で公開しています。

## 用途

### デスクトップアプリ（macOS・Windows・Linux）

- **毎日のオフィス ソフトとして**：Word・Excel・PowerPoint の文書を作り、編集し、Office と同じ形式で保存する。
- **受け取った文書をすぐ確かめる**：メールやチャットで届いた文書を、待たずに開いて、Office で作ったとおりの見た目で読む。大きなブックや、スライドの多いプレゼンテーションも速く開く。
- **PDF・画像への書き出し**：画面を出さずに、コマンドラインから PDF や PNG に書き出す。文書の変換をまとめて行う処理や、サーバーでの自動処理に組み込める。

### Web 版（WebAssembly ＋ WebGPU）

- **インストールせずに使う**：ブラウザで開くだけで、デスクトップアプリと同じ画面・同じ見た目で文書を表示し、編集する。ソフトを入れられない PC や、共有の端末でも使える。
- **Web システムに文書の表示と編集を組み込む**：業務システム・SaaS・ポータルの中で、添付された Word・Excel・PowerPoint の文書を、その場で Office と同じ見た目で表示・編集する。
- **文書をサーバーに送らずに扱う**：描画も編集もブラウザの中で行うので、手元のファイルを開くときは、文書の中身を外部のサーバーに送らない。

## ビルド

```bash
git clone --recurse-submodules https://github.com/ibisjp/ibis-office.git
cd ibis-office
cargo build --release
```

- Rust の最新の stable が要ります。
- macOS のアプリは `scripts/bundle-macos.sh` で、`target/release/ibisOffice.app` に作られます。
- ブラウザ版は `scripts/build-web.sh --release` でビルドし、`www/` を配信します（WebGPU に対応したブラウザ：Chrome・Edge・Safari 26 以降）。

### Linux

#### リリースの導入

このフォークは Linux 版を [Releases](https://github.com/lunagoat/ibis-office/releases/latest) で公開しています。最新のリリースをダウンロードして、いまのユーザー向けに導入するには（`~/.local` に入ります。root は要りません）：

```bash
curl -fsSL https://raw.githubusercontent.com/lunagoat/ibis-office/HEAD/scripts/install-linux.sh | sh
```

このスクリプト（`scripts/install-linux.sh`）は、`ibisOffice-<版>-Linux-x86_64.tar.gz` とその `.sha256` をダウンロードし、tarball をハッシュと照合してから一時フォルダに展開し、中の `install.sh` を実行します。手作業でも同じことができます。2 つのファイルをダウンロードし、tarball を展開して `ibisOffice/install.sh` を実行してください（「パッケージと導入」を参照）。すべてのユーザー向けに導入するには、root で実行します：

```bash
curl -fsSL https://raw.githubusercontent.com/lunagoat/ibis-office/HEAD/scripts/install-linux.sh | sudo sh -s -- --prefix /usr/local
```

アプリケーション メニューから、または `~/.local/bin` が `PATH` にあれば `ibisOffice` で起動します。いまのユーザー向けに導入したものは自動で更新されます（「自動更新」を参照）。同じコマンドをもう一度実行しても、最新のリリースが導入されます。削除するには `~/.local/lib/ibisOffice/uninstall.sh` を実行します。

リリースは Ubuntu 22.04 上で x86_64 向けにビルドしています。glibc 2.35 以降のディストリビューションで動きます。Ubuntu 22.04、Debian 12、Fedora 36 とそれ以降、および Arch Linux のようなローリング リリースのディストリビューションです。musl のディストリビューション（Alpine Linux）では動きません。ほかのプロセッサ向けには、ソースからビルドしてください。

次のライブラリが必要です。GStreamer とその基本プラグインのライブラリ、GLib、libxcb、libxkbcommon、libxkbcommon-x11、Vulkan ローダーと、お使いのグラフィックス カードの Vulkan ドライバーです。`install.sh` はこれらを探し、足りないものの名前と、それを導入するコマンドを表示します。まとめて導入するには：

- Debian / Ubuntu：`sudo apt install libgstreamer1.0-0 libgstreamer-plugins-base1.0-0 libglib2.0-0 libxcb1 libxkbcommon0 libxkbcommon-x11-0 libvulkan1`
- Fedora：`sudo dnf install gstreamer1 gstreamer1-plugins-base glib2 libxcb libxkbcommon libxkbcommon-x11 vulkan-loader`
- Arch Linux：`sudo pacman -S --needed gstreamer gst-plugins-base-libs glib2 libxcb libxkbcommon libxkbcommon-x11 vulkan-icd-loader`

試した範囲：この方法で（素の Ubuntu 22.04 の中で）ビルドした tarball を、必要なライブラリが 1 つも入っていない Ubuntu 22.04、Ubuntu 24.04、Fedora 44 の基本イメージと、CachyOS（Arch 系）に、上のスクリプトで導入しました。前の 3 つでは、`install.sh` が足りないライブラリを挙げ、表示されたコマンドでそれらが入り、そのあと `ibisOffice --export png` が Mesa のソフトウェア Vulkan ドライバーでスライドを描きました。CachyOS では足りないものはなく、同じ書き出しがグラフィックス カードで動きました。Debian、openSUSE などほかのディストリビューションは試していません。アプリのウィンドウを試したのは CachyOS（Hyprland）だけです。

#### ビルドに必要なパッケージ

Arch Linux：

```bash
sudo pacman -S --needed base-devel git python fontconfig freetype2 \
  libxcb libxkbcommon libxkbcommon-x11 wayland vulkan-icd-loader \
  glib2 gstreamer gst-plugins-base-libs
```

Debian / Ubuntu：

```bash
sudo apt install build-essential pkg-config git python3 libfontconfig-dev \
  libfreetype-dev libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev \
  libwayland-dev libvulkan1 libglib2.0-dev \
  libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev
```

Rust の最新の stable が要ります。`python3` は `scripts/bundle-linux.sh`（サードパーティのライセンス一覧の生成）に使います。実行には、グラフィックス カードの Vulkan ドライバーも要ります。

Debian / Ubuntu の一覧は、リリースのワークフロー（`.github/workflows/release-linux.yml`）が Ubuntu 22.04 でリリースをビルドするときに入れるものです。Arch の一覧は、ビルドがリンクまたは読み込むライブラリです。それらが導入済みの Arch 系の環境で確かめたもので、新規に導入した環境では試していません。

#### ビルド

```bash
cargo build --release -p ibis-office-app -p ibis-office-updater
```

実行ファイルは `target/release/ibisOffice` と `target/release/ibisOfficeUpdater`（自動更新が実行するプログラム）に作られます。

#### GStreamer なしのビルド

GStreamer（と開発用ファイル）なしでビルドするには：

```bash
cargo build --release -p ibis-office-app --no-default-features --features export
```

プレゼンテーションの動画・音声は再生されません。それ以外はすべて動きます。

#### パッケージと導入

tarball を作る：

```bash
scripts/bundle-linux.sh
```

`target/release/ibisOffice-<版>-Linux-<arch>.tar.gz` と、その `.sha256` が作られます。`<版>` は `version.txt` の `Linux` の行の版号で、`<arch>` は `uname -m`（例：`x86_64`）です。`scripts/bundle-linux.sh --no-build` は、`target/release` にある実行ファイルをそのままパッケージします。

展開した tarball から導入する：

```bash
tar -xzf ibisOffice-*-Linux-*.tar.gz
cd ibisOffice

# このユーザーに導入（root は不要）：
./install.sh

# すべてのユーザーに導入（root で実行）：
./install.sh --prefix /usr/local
```

`install.sh` はフォルダを `<prefix>/lib/ibisOffice/` にコピーし、`<prefix>/bin/ibisOffice` に実行ファイルへのリンクを作り、`.desktop` エントリとアイコンを XDG のデータ ディレクトリに入れ、`uninstall.sh` のためのマニフェストを書きます。`--prefix` を付けない場合、`<prefix>` は `~/.local` です。

削除するには：

```bash
<prefix>/lib/ibisOffice/uninstall.sh
```

ユーザーの設定とファイル（`~/.config/ibisOffice`、`~/.local/share/ibisOffice` など）はそのまま残ります。

#### 実行時に必要なもの

**スペルチェック。** ibisOffice は Hunspell の辞書（`.aff` と `.dic`）を読みます。探す場所は、`$DICPATH` に書かれたフォルダ、`$XDG_DATA_HOME/hunspell`（`~/.local/share/hunspell`）、`$XDG_DATA_DIRS` の各ディレクトリの `hunspell`・`myspell`・`myspell/dicts`（既定は `/usr/local/share` と `/usr/share`）です。書く言語の辞書を入れてください。

- Arch Linux：`hunspell-en_us`（ほかの `hunspell-*` パッケージも同様）
- Debian / Ubuntu：`hunspell-en-us`（ほかの `hunspell-*` パッケージも同様）

「辞書に追加」した語は、設定フォルダの `user-dictionary.txt`（`~/.config/ibisOffice/user-dictionary.txt`）に保存されます。

**動画・音声。** 再生は GStreamer の `playbin` で行います。システムの GStreamer と、そのクリップのコーデックのプラグインが必要です。

- Arch Linux：`gstreamer gst-plugins-base gst-plugins-good`（さらに `gst-plugins-bad`・`gst-plugins-ugly`・`gst-libav` でコーデックを追加）
- Debian / Ubuntu：`gstreamer1.0-plugins-base gstreamer1.0-plugins-good`（さらに `gstreamer1.0-plugins-bad`・`gstreamer1.0-plugins-ugly`・`gstreamer1.0-libav`）

コーデックのプラグインがないクリップは再生されません。不足しているプラグインは GStreamer がログに書きます。

**フォント。** Carlito（Calibri と同じ字幅のフォント）は実行ファイルに含まれています。Office のほかの書体は、ユーザーが入れない限り Linux にはありません。次のフォントが代わりに使われ、Office と同じ位置で行が折り返されます。

| Office のフォント | 代わりのフォント | Arch Linux パッケージ | Debian / Ubuntu パッケージ |
|---|---|---|---|
| Arial・Helvetica | Liberation Sans（または Arimo） | `ttf-liberation` | `fonts-liberation` |
| Times New Roman・Century | Liberation Serif（または Tinos） | `ttf-liberation` | `fonts-liberation` |
| Courier New | Liberation Mono（または Cousine） | `ttf-liberation` | `fonts-liberation` |
| Cambria | Caladea | `ttf-caladea` | `fonts-crosextra-caladea` |
| ＭＳ ゴシック | IPAGothic | `otf-ipafont` | `fonts-ipafont-gothic` |
| ＭＳ Ｐゴシック | IPAPGothic | `otf-ipafont` | `fonts-ipafont-gothic` |
| ＭＳ 明朝 | IPAMincho | `otf-ipafont` | `fonts-ipafont-mincho` |
| ＭＳ Ｐ明朝 | IPAPMincho | `otf-ipafont` | `fonts-ipafont-mincho` |

まとめて入れる：

- Arch Linux：`sudo pacman -S ttf-liberation ttf-caladea otf-ipafont`
- Debian / Ubuntu：`sudo apt install fonts-liberation fonts-crosextra-caladea fonts-ipafont`

#### 自動更新

起動時に GitHub のリポジトリで新しい版を確かめます（設定 ＞「起動時に更新を確かめる」、既定でオン）。そのリポジトリのブランチの `version.txt` を読み、対応するリリース（タグ `v<版>`）から tarball（`ibisOffice-<版>-Linux-<arch>.tar.gz`）とその `.sha256` をダウンロードし、`sha256sum` でハッシュを検証してから、`ibisOfficeUpdater` が導入されたフォルダを置き換えます。`curl`・`tar`・`sha256sum` が `PATH` に必要です。

更新のログは `$XDG_STATE_HOME/ibisOffice/updater.log`（既定は `~/.local/state/ibisOffice/updater.log`）にあります。ヘルプ ＞ 更新のログを表示 で開きます。

`target/` から実行した開発ビルド、書き込めないフォルダにある導入（`--prefix` で入れたもの）、`ibisOffice` フォルダの中にいない実行ファイルからは、更新は行われません。同じ版で 2 回失敗すると、その版は提示されなくなります。

このフォークのリリースは `lunagoat/ibis-office` を見に行き、その既定のブランチ（`linux`）の `version.txt` を読みます。自分でビルドしたものは、ほかを指定しない限り、ibis inc. のリポジトリ `ibisjp/ibis-office` の `main` ブランチを見に行きます。

**フォークの管理者向け：** ビルド時にリポジトリを指定します。

```bash
IBIS_UPDATE_REPO=owner/repo cargo build --release -p ibis-office-app -p ibis-office-updater
```

または：

```bash
IBIS_UPDATE_REPO=owner/repo scripts/bundle-linux.sh
```

`version.txt` のあるブランチが `main` でない場合は `IBIS_UPDATE_BRANCH` も指定します。上流のリポジトリ（`ibisjp/ibis-office`）の `version.txt` には `Linux` の行がないので、それを見に行くビルドには何も提示されません。

更新と `scripts/install-linux.sh` が見つけるリリースの作り方：

1. `version.txt` の `Linux` の行を新しい版号（数字とドットだけ）に変え、コミットして既定のブランチにプッシュする。このフォークはリリースに `<上流の版>.<n>` という番号を付けます（`0.8.1.1` は上流の 0.8.1 に対する最初のリリース）。タグが上流のものと重ならないようにするためです。
2. そのコミットに `v<版>` のタグを付け、タグをプッシュする。

タグをプッシュすると、リリースのワークフロー（`.github/workflows/release-linux.yml`）が始まります。Ubuntu 22.04 で、`IBIS_UPDATE_REPO` を実行中のリポジトリに、`IBIS_UPDATE_BRANCH` をその既定のブランチにして tarball をビルドし、確認として導入してプレゼンテーションを書き出したうえで、`ibisOffice-<版>-Linux-x86_64.tar.gz` とその `.sha256` を GitHub のリリース `v<版>` に添付します。ワークフローはリポジトリの Actions ページからも始められます。その場合、タグはワークフローが作ります。ワークフローを使わずに公開するには、`scripts/bundle-linux.sh` が作る 2 つのファイルを、`v<版>` のタグが付いたリリースに添付します。

このリポジトリをフォークした場合は、`scripts/install-linux.sh` の先頭の `REPO` と、この節のアドレスも自分のリポジトリに変えてください。

#### コマンドラインでの書き出しと印刷

`ibisOffice --export png|pdf|pdf-image|ui` はディスプレイ サーバーなしで動きます（`export` フィーチャー付きのビルド。既定でオン）。印刷（ファイル ＞ 印刷）は XDG デスクトップ ポータルの印刷ダイアログを使います。ポータルが印刷に対応していない場合は、PDF をシステムの PDF ビューアーで開きます。

#### タイトルバー

ibisOffice はデスクトップにウィンドウのタイトルバーを描かせます（サーバーサイド デコレーション）。KDE・wlroots コンポジター・Hyprland・X11 のウィンドウ マネージャーは、ウィンドウの装飾を自分で描きます（Hyprland のようなタイル型コンポジターでは、タイトルバーのない枠だけです）。GNOME の Wayland セッションは描かないので、ibisOffice がタイトルバーを自分で描きます（クライアントサイド デコレーション）。`IBIS_WINDOW_DECORATIONS=client` を指定すると、どのデスクトップでも ibisOffice が自分のタイトルバーを描きます（デスクトップが自前の装飾を使い続ける場合、ウィンドウのボタンは付きません）。既定は `server` です。この方法で Hyprland では表示を確かめましたが、GNOME では試していません。

#### 既知の制限

- シナリオ テスト（`cargo test --release -p ibis-office-app --test scenarios`）は Linux で動きますが、923 件のうち 875 件はスキップされます。開く文書（`test/editing/`）が公開リポジトリにないためです。残りの 48 件は通ります。
- デバッグ ビルドは一部のファイルで上流の `debug_assert!` に引っかかります。リリース ビルドを使ってください。
- 公開しているリリースは x86_64、glibc 2.35 以降向けです。aarch64 向けと musl 向けのビルドはありません。
- 自分でビルドしたものは、Linux のリリースを公開するリポジトリを（`IBIS_UPDATE_REPO` で）指定しない限り、更新を提示されません。
- 動画・音声にはクリップのコーデックの GStreamer プラグインが必要です。プラグインがないクリップは再生されません。
- GNOME の Wayland セッションでアプリが描くクライアントサイドのタイトルバーは、GNOME では試されていません。

## 実装済み

### docx

| # | 内容 | <nobr>状態</nobr> |
|---|---|---|
| 1 | docx: 文字入力と削除 | <nobr>完了</nobr> |
| 2 | docx: IME変換と確定 | <nobr>完了</nobr> |
| 3 | docx: 単語・段落・全文選択 | <nobr>完了</nobr> |
| 4 | docx: 段落分割と結合 | <nobr>完了</nobr> |
| 5 | docx: 手動改行とタブの入力 | <nobr>完了</nobr> |
| 6 | docx: 切り取り・コピー | <nobr>完了</nobr> |
| 7 | docx: 元の書式を保持して貼付 | <nobr>完了</nobr> |
| 8 | docx: 貼付先の書式へ統合 | <nobr>完了</nobr> |
| 9 | docx: テキストのみ貼付 | <nobr>完了</nobr> |
| 10 | docx: 形式を選択して貼付 | <nobr>完了</nobr> |
| 11 | docx: 書式のコピーと反復適用 | <nobr>完了</nobr> |
| 12 | docx: 元に戻すとやり直し | <nobr>完了</nobr> |
| 13 | docx: 特殊文字・記号の挿入 | <nobr>完了</nobr> |
| 14 | docx: 日付時刻の挿入と自動更新 | <nobr>完了</nobr> |
| 15 | docx: 欧文・和文・複雑文字用フォント | <nobr>一部</nobr> |
| 16 | docx: フォントサイズと増減 | <nobr>完了</nobr> |
| 17 | docx: 太字と斜体 | <nobr>完了</nobr> |
| 18 | docx: 下線の種類と色 | <nobr>完了</nobr> |
| 19 | docx: 取り消し線と二重取り消し線 | <nobr>完了</nobr> |
| 20 | docx: 上付き・下付き | <nobr>完了</nobr> |
| 21 | docx: 文字色とテーマ色 | <nobr>完了</nobr> |
| 22 | docx: 蛍光ペン | <nobr>完了</nobr> |
| 23 | docx: 文字の網かけと囲い線 | <nobr>完了</nobr> |
| 24 | docx: 大文字・小型英大文字 | <nobr>完了</nobr> |
| 25 | docx: 隠し文字 | <nobr>完了</nobr> |
| 26 | docx: 文字幅の拡大縮小 | <nobr>完了</nobr> |
| 27 | docx: 文字間隔の詰めと広げ | <nobr>完了</nobr> |
| 28 | docx: 文字位置の上げ下げ | <nobr>完了</nobr> |
| 29 | docx: カーニング | <nobr>完了</nobr> |
| 30 | docx: 文字の輪郭・影・光彩 | <nobr>一部</nobr> |
| 31 | docx: 直接書式の解除 | <nobr>完了</nobr> |
| 32 | docx: 左・中央・右・両端揃え | <nobr>完了</nobr> |
| 33 | docx: 均等割付 | <nobr>完了</nobr> |
| 34 | docx: 左右インデント | <nobr>完了</nobr> |
| 35 | docx: 字数単位インデント | <nobr>完了</nobr> |
| 36 | docx: 先頭行とぶら下げインデント | <nobr>完了</nobr> |
| 37 | docx: 段落前後の間隔 | <nobr>完了</nobr> |
| 38 | docx: 同一スタイル間の間隔抑制 | <nobr>完了</nobr> |
| 39 | docx: 単一・倍数行間 | <nobr>完了</nobr> |
| 40 | docx: 固定値・最小値行間 | <nobr>完了</nobr> |
| 41 | docx: タブ位置・配置・リーダー | <nobr>完了</nobr> |
| 42 | docx: 段落罫線 | <nobr>完了</nobr> |
| 43 | docx: 段落網かけ | <nobr>完了</nobr> |
| 44 | docx: 孤立行防止 | <nobr>完了</nobr> |
| 45 | docx: 次段落と分離しない | <nobr>完了</nobr> |
| 46 | docx: 段落内で改ページしない | <nobr>完了</nobr> |
| 47 | docx: 段落前で改ページ | <nobr>完了</nobr> |
| 48 | docx: 行番号の対象除外 | <nobr>完了</nobr> |
| 49 | docx: ハイフネーションの対象除外 | <nobr>完了</nobr> |
| 50 | docx: 記号の箇条書き | <nobr>完了</nobr> |
| 51 | docx: 番号の書式 | <nobr>完了</nobr> |
| 52 | docx: 開始番号の指定 | <nobr>完了</nobr> |
| 53 | docx: 番号の再開始と継続 | <nobr>完了</nobr> |
| 54 | docx: 複数レベルのリスト | <nobr>完了</nobr> |
| 55 | docx: レベルごとの番号連結 | <nobr>完了</nobr> |
| 56 | docx: 見出しスタイルと番号の連動 | <nobr>完了</nobr> |
| 57 | docx: 番号の配置と字下げ | <nobr>完了</nobr> |
| 58 | docx: 番号後のタブ・空白 | <nobr>完了</nobr> |
| 59 | docx: リストのレベル変更 | <nobr>完了</nobr> |
| 60 | docx: 段落スタイルの適用 | <nobr>完了</nobr> |
| 61 | docx: 文字スタイルの適用 | <nobr>完了</nobr> |
| 62 | docx: リンクスタイルの適用 | <nobr>完了</nobr> |
| 63 | docx: 表スタイルの適用 | <nobr>完了</nobr> |
| 64 | docx: スタイルの新規作成 | <nobr>完了</nobr> |
| 65 | docx: 既存スタイルの変更 | <nobr>完了</nobr> |
| 66 | docx: 基準スタイルと継承 | <nobr>完了</nobr> |
| 67 | docx: 次段落スタイル | <nobr>完了</nobr> |
| 68 | docx: 選択書式からスタイル更新 | <nobr>完了</nobr> |
| 69 | docx: 文書テーマ | <nobr>一部</nobr> |
| 70 | docx: 表の挿入 | <nobr>完了</nobr> |
| 71 | docx: 行列の追加と削除 | <nobr>完了</nobr> |
| 72 | docx: セル追加とシフト | <nobr>完了</nobr> |
| 73 | docx: セル結合 | <nobr>完了</nobr> |
| 74 | docx: セル分割 | <nobr>完了</nobr> |
| 75 | docx: 表分割 | <nobr>完了</nobr> |
| 76 | docx: 入れ子の表 | <nobr>完了</nobr> |
| 77 | docx: 文字列から表へ変換 | <nobr>完了</nobr> |
| 78 | docx: 表から文字列へ変換 | <nobr>完了</nobr> |
| 79 | docx: 表内の並べ替え | <nobr>完了</nobr> |
| 80 | docx: 表内の数式 | <nobr>完了</nobr> |
| 81 | docx: セル内の段落とリスト | <nobr>完了</nobr> |
| 82 | docx: 表・行・列・セル選択 | <nobr>完了</nobr> |
| 83 | docx: 表の位置揃えと字下げ | <nobr>完了</nobr> |
| 84 | docx: 固定幅・割合幅の表 | <nobr>完了</nobr> |
| 85 | docx: 列幅と行高 | <nobr>完了</nobr> |
| 86 | docx: 行高の固定値・最小値 | <nobr>完了</nobr> |
| 87 | docx: 内容に合わせる自動調整 | <nobr>完了</nobr> |
| 88 | docx: ウィンドウに合わせる自動調整 | <nobr>完了</nobr> |
| 89 | docx: 列幅・行高の均等化 | <nobr>完了</nobr> |
| 90 | docx: セル余白 | <nobr>完了</nobr> |
| 91 | docx: セル間隔 | <nobr>完了</nobr> |
| 92 | docx: セル内の上下配置 | <nobr>完了</nobr> |
| 93 | docx: セル内文字の方向 | <nobr>完了</nobr> |
| 94 | docx: 表の罫線・線幅・色 | <nobr>完了</nobr> |
| 95 | docx: 表の塗りつぶし | <nobr>完了</nobr> |
| 96 | docx: 見出し行・集計行の表スタイル | <nobr>完了</nobr> |
| 97 | docx: 行列の縞模様 | <nobr>完了</nobr> |
| 98 | docx: 先頭列・最終列の強調 | <nobr>完了</nobr> |
| 99 | docx: ページごとの見出し行反復 | <nobr>完了</nobr> |
| 100 | docx: 行をページ途中で分割 | <nobr>完了</nobr> |
| 101 | docx: 浮動表と周囲からの距離 | <nobr>完了</nobr> |
| 102 | docx: 画像の挿入と置換 | <nobr>完了</nobr> |
| 103 | docx: 画像サイズと縦横比固定 | <nobr>完了</nobr> |
| 104 | docx: 画像の回転・反転 | <nobr>完了</nobr> |
| 105 | docx: 画像の切り抜き | <nobr>完了</nobr> |
| 106 | docx: 画像枠線と効果 | <nobr>完了</nobr> |
| 107 | docx: SVGの挿入 | <nobr>完了</nobr> |
| 108 | docx: テキストボックス | <nobr>一部</nobr> |
| 109 | docx: グラフと埋込データ | <nobr>一部</nobr> |
| 110 | docx: 行内配置 | <nobr>完了</nobr> |
| 111 | docx: 四角形の折返し | <nobr>完了</nobr> |
| 112 | docx: 外周・内部の折返し | <nobr>完了</nobr> |
| 113 | docx: 上下のみの折返し | <nobr>完了</nobr> |
| 114 | docx: 文字の背面と前面 | <nobr>完了</nobr> |
| 115 | docx: 文字との距離 | <nobr>完了</nobr> |
| 116 | docx: 水平位置の基準と相対位置 | <nobr>完了</nobr> |
| 117 | docx: 垂直位置の基準と相対位置 | <nobr>完了</nobr> |
| 118 | docx: 本文と一緒に移動 | <nobr>完了</nobr> |
| 119 | docx: ページ上の位置固定 | <nobr>完了</nobr> |
| 120 | docx: アンカー固定 | <nobr>完了</nobr> |
| 121 | docx: オブジェクトの重なり許可 | <nobr>完了</nobr> |
| 122 | docx: 前後の重なり順 | <nobr>完了</nobr> |
| 123 | docx: 整列と均等配置 | <nobr>完了</nobr> |
| 124 | docx: オブジェクト名と選択ウィンドウ | <nobr>完了</nobr> |
| 125 | docx: 表のセル内で配置する | <nobr>完了</nobr> |
| 126 | docx: ルビの入力 | <nobr>完了</nobr> |
| 127 | docx: ルビの対象範囲と配置 | <nobr>完了</nobr> |
| 128 | docx: ルビ書式と本文書式の分離 | <nobr>完了</nobr> |
| 129 | docx: 傍点 | <nobr>完了</nobr> |
| 130 | docx: 囲み文字 | <nobr>完了</nobr> |
| 131 | docx: 組み文字 | <nobr>完了</nobr> |
| 132 | docx: 縦中横 | <nobr>完了</nobr> |
| 133 | docx: 縦書きと横書き | <nobr>完了</nobr> |
| 134 | docx: 禁則処理 | <nobr>完了</nobr> |
| 135 | docx: 句読点のぶら下げ | <nobr>完了</nobr> |
| 136 | docx: 文字間隔の自動調整 | <nobr>完了</nobr> |
| 137 | docx: 和欧文間の空き | <nobr>完了</nobr> |
| 138 | docx: 和文と数字間の空き | <nobr>完了</nobr> |
| 139 | docx: 文字数・行数の原稿設定 | <nobr>完了</nobr> |
| 140 | docx: 文書グリッドへの配置 | <nobr>完了</nobr> |
| 141 | docx: 右から左への段落方向 | <nobr>完了</nobr> |
| 142 | docx: 複雑文字のフォントとサイズ | <nobr>一部</nobr> |
| 143 | docx: 全角半角変換 | <nobr>完了</nobr> |
| 144 | docx: 用紙サイズ | <nobr>完了</nobr> |
| 145 | docx: 縦・横の用紙方向 | <nobr>完了</nobr> |
| 146 | docx: 上下左右余白 | <nobr>完了</nobr> |
| 147 | docx: 見開き余白 | <nobr>完了</nobr> |
| 148 | docx: とじしろと位置 | <nobr>完了</nobr> |
| 149 | docx: 本文の垂直配置 | <nobr>完了</nobr> |
| 150 | docx: 改ページ | <nobr>完了</nobr> |
| 151 | docx: 次ページのセクション区切り | <nobr>完了</nobr> |
| 152 | docx: 連続セクション区切り | <nobr>完了</nobr> |
| 153 | docx: 奇数・偶数ページの区切り | <nobr>完了</nobr> |
| 154 | docx: セクション単位の用紙設定 | <nobr>完了</nobr> |
| 155 | docx: 段組の段数 | <nobr>完了</nobr> |
| 156 | docx: 段ごとの幅と間隔 | <nobr>完了</nobr> |
| 157 | docx: 段間罫線 | <nobr>完了</nobr> |
| 158 | docx: 段区切り | <nobr>完了</nobr> |
| 159 | docx: 行番号の開始・再開始 | <nobr>完了</nobr> |
| 160 | docx: ページ罫線 | <nobr>完了</nobr> |
| 161 | docx: ページ色 | <nobr>完了</nobr> |
| 162 | docx: ヘッダーとフッターの編集 | <nobr>完了</nobr> |
| 163 | docx: 先頭ページを別指定 | <nobr>完了</nobr> |
| 164 | docx: 奇数偶数ページを別指定 | <nobr>完了</nobr> |
| 165 | docx: 前セクションとのリンク | <nobr>完了</nobr> |
| 166 | docx: 上下端からの位置 | <nobr>完了</nobr> |
| 167 | docx: ヘッダー内の画像・表 | <nobr>一部</nobr> |
| 168 | docx: フッター内のフィールド | <nobr>完了</nobr> |
| 169 | docx: ページ番号の位置 | <nobr>完了</nobr> |
| 170 | docx: ページ番号の書式 | <nobr>完了</nobr> |
| 171 | docx: ページ番号の開始値 | <nobr>完了</nobr> |
| 172 | docx: セクションごとの番号継続 | <nobr>完了</nobr> |
| 173 | docx: 総ページ数 | <nobr>完了</nobr> |
| 174 | docx: ヘッダーのスタイル参照 | <nobr>一部</nobr> |
| 175 | docx: フィールド挿入とコード編集 | <nobr>完了</nobr> |
| 176 | docx: フィールドコードと結果表示 | <nobr>完了</nobr> |
| 177 | docx: 選択範囲・文書全体の更新 | <nobr>完了</nobr> |
| 178 | docx: フィールドのロック | <nobr>完了</nobr> |
| 179 | docx: フィールドのリンク解除 | <nobr>完了</nobr> |
| 180 | docx: フィールドの入れ子 | <nobr>完了</nobr> |
| 181 | docx: 数値・文字・日付の書式スイッチ | <nobr>完了</nobr> |
| 182 | docx: PAGE・NUMPAGES・SECTIONPAGES | <nobr>完了</nobr> |
| 183 | docx: DATE・TIME・CREATEDATE・SAVEDATE | <nobr>完了</nobr> |
| 184 | docx: AUTHOR・TITLE・DOCPROPERTY | <nobr>完了</nobr> |
| 185 | docx: REF・PAGEREF・NOTEREF | <nobr>完了</nobr> |
| 186 | docx: SEQ・LISTNUM | <nobr>完了</nobr> |
| 187 | docx: STYLEREF | <nobr>完了</nobr> |
| 188 | docx: IF・COMPARE | <nobr>完了</nobr> |
| 189 | docx: ASK・FILLIN | <nobr>完了</nobr> |
| 190 | docx: 表のFormulaフィールド | <nobr>完了</nobr> |
| 191 | docx: 文書プロパティとカスタム属性 | <nobr>完了</nobr> |
| 192 | docx: 脚注の挿入と削除 | <nobr>完了</nobr> |
| 193 | docx: 文末脚注の挿入と削除 | <nobr>完了</nobr> |
| 194 | docx: 脚注と文末脚注の変換 | <nobr>完了</nobr> |
| 195 | docx: 注番号の書式と開始値 | <nobr>完了</nobr> |
| 196 | docx: 注番号の再開始 | <nobr>完了</nobr> |
| 197 | docx: 注の配置 | <nobr>完了</nobr> |
| 198 | docx: 注の区切り線 | <nobr>一部</nobr> |
| 199 | docx: ブックマーク | <nobr>完了</nobr> |
| 200 | docx: 内部・外部ハイパーリンク | <nobr>完了</nobr> |
| 201 | docx: 相互参照 | <nobr>完了</nobr> |
| 202 | docx: 図表番号とラベル | <nobr>完了</nobr> |
| 203 | docx: 見出しから自動目次 | <nobr>完了</nobr> |
| 204 | docx: 目次の見出しレベル | <nobr>完了</nobr> |
| 205 | docx: 任意スタイルを目次へ対応 | <nobr>完了</nobr> |
| 206 | docx: 目次のタブリーダーとページ番号 | <nobr>完了</nobr> |
| 207 | docx: 目次全体と番号だけの更新 | <nobr>完了</nobr> |
| 208 | docx: 図表目次 | <nobr>完了</nobr> |
| 209 | docx: ナビゲーションペイン | <nobr>完了</nobr> |
| 210 | docx: リッチテキストコンテンツコントロール | <nobr>一部</nobr> |
| 211 | docx: プレーンテキストコントロール | <nobr>一部</nobr> |
| 212 | docx: チェックボックスコントロール | <nobr>一部</nobr> |
| 213 | docx: コンボボックス | <nobr>一部</nobr> |
| 214 | docx: ドロップダウン一覧 | <nobr>一部</nobr> |
| 215 | docx: 日付選択コントロール | <nobr>一部</nobr> |
| 216 | docx: 文書パーツギャラリーコントロール | <nobr>一部</nobr> |
| 217 | docx: プレースホルダーの案内文 | <nobr>一部</nobr> |
| 218 | docx: 変更履歴の記録 | <nobr>完了</nobr> |
| 219 | docx: 挿入・削除の変更履歴 | <nobr>完了</nobr> |
| 220 | docx: 書式の変更履歴 | <nobr>完了</nobr> |
| 221 | docx: 移動と表変更の履歴 | <nobr>一部</nobr> |
| 222 | docx: 著者別の変更表示 | <nobr>完了</nobr> |
| 223 | docx: 変更表示の簡易・全体・原文切替 | <nobr>完了</nobr> |
| 224 | docx: 変更履歴の承諾 | <nobr>完了</nobr> |
| 225 | docx: 表示中の変更だけ処理 | <nobr>完了</nobr> |
| 226 | docx: 変更履歴の記録をロック | <nobr>完了</nobr> |
| 227 | docx: コメント追加と削除 | <nobr>完了</nobr> |
| 228 | docx: コメントへの返信 | <nobr>完了</nobr> |
| 229 | docx: コメントの解決と再開 | <nobr>完了</nobr> |
| 230 | docx: アクセシビリティ検査 | <nobr>完了</nobr> |
| 231 | docx: 画像の代替テキスト | <nobr>完了</nobr> |
| 232 | docx: 装飾画像の指定 | <nobr>完了</nobr> |
| 233 | docx: 見出し階層 | <nobr>完了</nobr> |
| 234 | docx: 表の見出し行指定 | <nobr>完了</nobr> |
| 235 | docx: 分かりやすいリンク表示 | <nobr>完了</nobr> |
| 236 | docx: キーボード操作 | <nobr>一部</nobr> |
| 237 | docx: 表示ズーム | <nobr>完了</nobr> |
| 238 | docx: ダークモード | <nobr>一部</nobr> |
| 239 | docx: 読取り専用の推奨 | <nobr>完了</nobr> |
| 240 | docx: 最終版としてマーク | <nobr>完了</nobr> |
| 241 | docx: 印刷プレビュー | <nobr>完了</nobr> |
| 242 | docx: PDF出力とタグ | <nobr>完了</nobr> |
| 243 | docx: Advanceフィールド | <nobr>完了</nobr> |
| 244 | docx: Askフィールド | <nobr>完了</nobr> |
| 245 | docx: Authorフィールド | <nobr>完了</nobr> |
| 246 | docx: AutoNumフィールド | <nobr>完了</nobr> |
| 247 | docx: AutoNumLglフィールド | <nobr>完了</nobr> |
| 248 | docx: AutoNumOutフィールド | <nobr>完了</nobr> |
| 249 | docx: Commentsフィールド | <nobr>完了</nobr> |
| 250 | docx: Compareフィールド | <nobr>完了</nobr> |
| 251 | docx: CreateDateフィールド | <nobr>完了</nobr> |
| 252 | docx: Dateフィールド | <nobr>完了</nobr> |
| 253 | docx: DocPropertyフィールド | <nobr>完了</nobr> |
| 254 | docx: DocVariableフィールド | <nobr>完了</nobr> |
| 255 | docx: Eqフィールド | <nobr>一部</nobr> |
| 256 | docx: FileNameフィールド | <nobr>完了</nobr> |
| 257 | docx: FileSizeフィールド | <nobr>完了</nobr> |
| 258 | docx: Fill-Inフィールド | <nobr>完了</nobr> |
| 259 | docx: = (Formula)フィールド | <nobr>完了</nobr> |
| 260 | docx: Hyperlinkフィールド | <nobr>完了</nobr> |
| 261 | docx: Ifフィールド | <nobr>完了</nobr> |
| 262 | docx: IncludePictureフィールド | <nobr>一部</nobr> |
| 263 | docx: IncludeTextフィールド | <nobr>一部</nobr> |
| 264 | docx: Indexフィールド | <nobr>一部</nobr> |
| 265 | docx: Infoフィールド | <nobr>完了</nobr> |
| 266 | docx: Keywordsフィールド | <nobr>完了</nobr> |
| 267 | docx: LastSavedByフィールド | <nobr>完了</nobr> |
| 268 | docx: ListNumフィールド | <nobr>完了</nobr> |
| 269 | docx: NoteRefフィールド | <nobr>完了</nobr> |
| 270 | docx: NumCharsフィールド | <nobr>完了</nobr> |
| 271 | docx: NumPagesフィールド | <nobr>完了</nobr> |
| 272 | docx: NumWordsフィールド | <nobr>完了</nobr> |
| 273 | docx: Pageフィールド | <nobr>完了</nobr> |
| 274 | docx: PageRefフィールド | <nobr>完了</nobr> |
| 275 | docx: PrintDateフィールド | <nobr>完了</nobr> |
| 276 | docx: Quoteフィールド | <nobr>完了</nobr> |
| 277 | docx: Refフィールド | <nobr>完了</nobr> |
| 278 | docx: RevNumフィールド | <nobr>完了</nobr> |
| 279 | docx: SaveDateフィールド | <nobr>完了</nobr> |
| 280 | docx: Sectionフィールド | <nobr>完了</nobr> |
| 281 | docx: SectionPagesフィールド | <nobr>完了</nobr> |
| 282 | docx: Seq (Sequence)フィールド | <nobr>完了</nobr> |
| 283 | docx: Setフィールド | <nobr>完了</nobr> |
| 284 | docx: StyleRefフィールド | <nobr>完了</nobr> |
| 285 | docx: Subjectフィールド | <nobr>完了</nobr> |
| 286 | docx: Symbolフィールド | <nobr>完了</nobr> |
| 287 | docx: TA (Table of Authorities Entry)フィールド | <nobr>一部</nobr> |
| 288 | docx: TC (Table of Contents Entry)フィールド | <nobr>一部</nobr> |
| 289 | docx: Templateフィールド | <nobr>完了</nobr> |
| 290 | docx: Timeフィールド | <nobr>完了</nobr> |
| 291 | docx: Titleフィールド | <nobr>完了</nobr> |
| 292 | docx: TOA (Table of Authorities)フィールド | <nobr>一部</nobr> |
| 293 | docx: TOC (Table of Contents)フィールド | <nobr>完了</nobr> |
| 294 | docx: UserAddressフィールド | <nobr>完了</nobr> |
| 295 | docx: UserInitialsフィールド | <nobr>完了</nobr> |
| 296 | docx: UserNameフィールド | <nobr>完了</nobr> |
| 297 | docx: XE (Index Entry)フィールド | <nobr>一部</nobr> |
| 298 | docx: OOXML標準とMicrosoft拡張の保存要素を対応 | <nobr>一部</nobr> |
| 299 | docx: 未対応機能の無編集往復保存と代替表示を評価 | <nobr>一部</nobr> |
| 300 | docx: 既存features/scenarios/coverageのIDを対応 | <nobr>一部</nobr> |
| 301 | docx: 印刷レイアウト表示 | <nobr>完了</nobr> |
| 302 | docx: アウトライン表示 | <nobr>一部</nobr> |
| 303 | docx: ページの縦方向移動 | <nobr>完了</nobr> |
| 304 | docx: ページの左右移動 | <nobr>一部</nobr> |
| 305 | docx: 1ページ・複数ページ・ページ幅表示 | <nobr>完了</nobr> |
| 306 | docx: ズーム倍率指定 | <nobr>完了</nobr> |
| 307 | docx: 水平ルーラー | <nobr>完了</nobr> |
| 308 | docx: 垂直ルーラー | <nobr>完了</nobr> |
| 309 | docx: ルーラーの単位設定 | <nobr>完了</nobr> |
| 310 | docx: グリッド線表示 | <nobr>完了</nobr> |
| 311 | docx: ナビゲーションの見出し・ページ・検索 | <nobr>完了</nobr> |
| 312 | docx: 編集記号の表示 | <nobr>完了</nobr> |
| 313 | docx: ページ間余白の表示 | <nobr>完了</nobr> |
| 314 | docx: 空白文書の新規作成 | <nobr>完了</nobr> |
| 315 | docx: 既存文書を開く | <nobr>完了</nobr> |
| 316 | docx: 最近使った文書と保存先 | <nobr>完了</nobr> |
| 317 | docx: 保存・名前を付けて保存 | <nobr>完了</nobr> |
| 318 | docx: コピーとして保存 | <nobr>完了</nobr> |
| 319 | docx: タイトル・件名・著者等の標準プロパティ | <nobr>完了</nobr> |
| 320 | docx: カスタムプロパティ | <nobr>完了</nobr> |
| 321 | docx: 文書統計と最終更新情報 | <nobr>完了</nobr> |
| 322 | docx: マクロ付きの文書（.docm）を、マクロを実行せずに開いて保存する | <nobr>完了</nobr> |
| 323 | docx: ユーザー名と校閲著者 | <nobr>完了</nobr> |
| 324 | docx: 上書き入力 | <nobr>完了</nobr> |
| 325 | docx: 貼付の既定動作 | <nobr>完了</nobr> |
| 326 | docx: 編集記号と隠し文字の表示 | <nobr>完了</nobr> |
| 327 | docx: フィールド・リンクの印刷前更新 | <nobr>完了</nobr> |
| 328 | docx: 背景色・画像・隠し文字の印刷 | <nobr>完了</nobr> |
| 329 | docx: ホーム／挿入／描画／デザイン／レイアウト／参考資料／差込／校閲／表示／Developerを照合 | <nobr>一部</nobr> |
| 330 | docx: 表・画像・図形・数式・ヘッダーのコンテキストタブと右クリックを照合 | <nobr>一部</nobr> |
| 331 | docx: 本文の文字列検索と次の一致 | <nobr>完了</nobr> |
| 332 | docx: 個別置換と一括置換 | <nobr>完了</nobr> |
| 333 | docx: 大文字小文字の区別 | <nobr>完了</nobr> |
| 334 | docx: 単語全体の一致 | <nobr>完了</nobr> |
| 335 | docx: ワイルドカードによる検索 | <nobr>完了</nobr> |
| 336 | docx: ワイルドカードのグループ参照による置換 | <nobr>完了</nobr> |
| 337 | docx: 特殊文字・段落記号の検索と置換 | <nobr>完了</nobr> |
| 338 | docx: 検索範囲と方向 | <nobr>完了</nobr> |
| 339 | docx: ページ・見出し・注等へのジャンプ | <nobr>完了</nobr> |

### xlsx

| # | 内容 | <nobr>状態</nobr> |
|---|---|---|
| 1 | xlsx: 数値・文字・論理値の入力 | <nobr>完了</nobr> |
| 2 | xlsx: 日付・時刻・通貨・割合の入力 | <nobr>完了</nobr> |
| 3 | xlsx: 長い数値と精度の扱い | <nobr>完了</nobr> |
| 4 | xlsx: セル内改行 | <nobr>完了</nobr> |
| 5 | xlsx: セルのリッチテキスト | <nobr>一部</nobr> |
| 6 | xlsx: 日本語入力とふりがな | <nobr>一部</nobr> |
| 7 | xlsx: セル・範囲・非連続範囲選択 | <nobr>完了</nobr> |
| 8 | xlsx: 切取りとコピー | <nobr>完了</nobr> |
| 9 | xlsx: 値・数式・書式の選択貼付 | <nobr>完了</nobr> |
| 10 | xlsx: 貼付時の転置 | <nobr>完了</nobr> |
| 11 | xlsx: 貼付時の四則演算 | <nobr>完了</nobr> |
| 12 | xlsx: 空白を無視する貼付 | <nobr>完了</nobr> |
| 13 | xlsx: 値・書式・コメントのクリア | <nobr>完了</nobr> |
| 14 | xlsx: オートフィル | <nobr>完了</nobr> |
| 15 | xlsx: 連続データの作成 | <nobr>完了</nobr> |
| 16 | xlsx: フラッシュフィル | <nobr>一部</nobr> |
| 17 | xlsx: 検索と置換 | <nobr>完了</nobr> |
| 18 | xlsx: 条件を選択してジャンプ | <nobr>完了</nobr> |
| 19 | xlsx: 元に戻すとやり直し | <nobr>完了</nobr> |
| 20 | xlsx: シート追加・削除 | <nobr>完了</nobr> |
| 21 | xlsx: シート名変更 | <nobr>完了</nobr> |
| 22 | xlsx: シート移動とコピー | <nobr>完了</nobr> |
| 23 | xlsx: シートタブ色 | <nobr>完了</nobr> |
| 24 | xlsx: シート非表示と再表示 | <nobr>完了</nobr> |
| 25 | xlsx: 行列の追加と削除 | <nobr>完了</nobr> |
| 26 | xlsx: 行高・列幅 | <nobr>完了</nobr> |
| 27 | xlsx: 行列の自動サイズ | <nobr>完了</nobr> |
| 28 | xlsx: 行列の非表示 | <nobr>完了</nobr> |
| 29 | xlsx: アウトラインのグループ化 | <nobr>完了</nobr> |
| 30 | xlsx: 小計とアウトライン | <nobr>完了</nobr> |
| 31 | xlsx: ウィンドウ枠固定 | <nobr>完了</nobr> |
| 32 | xlsx: 表示倍率 | <nobr>完了</nobr> |
| 33 | xlsx: シートのグリッド線表示 | <nobr>完了</nobr> |
| 34 | xlsx: 見出し表示 | <nobr>完了</nobr> |
| 35 | xlsx: ウィンドウ分割 | <nobr>完了</nobr> |
| 36 | xlsx: 複数ブックの整列 | <nobr>完了</nobr> |
| 37 | xlsx: 数式バー表示 | <nobr>完了</nobr> |
| 38 | xlsx: 標準・ページレイアウト・改ページプレビュー | <nobr>完了</nobr> |
| 39 | xlsx: フォントとサイズ | <nobr>完了</nobr> |
| 40 | xlsx: 太字・斜体・下線 | <nobr>完了</nobr> |
| 41 | xlsx: 取り消し線・上付き・下付き | <nobr>完了</nobr> |
| 42 | xlsx: 文字色とテーマ色 | <nobr>一部</nobr> |
| 43 | xlsx: 水平・垂直配置 | <nobr>完了</nobr> |
| 44 | xlsx: 折返し表示 | <nobr>完了</nobr> |
| 45 | xlsx: 縮小して全体表示 | <nobr>完了</nobr> |
| 46 | xlsx: 文字の回転 | <nobr>完了</nobr> |
| 47 | xlsx: 縦書き | <nobr>完了</nobr> |
| 48 | xlsx: 字下げ | <nobr>完了</nobr> |
| 49 | xlsx: セル結合と中央揃え | <nobr>完了</nobr> |
| 50 | xlsx: 罫線の種類・線幅・色 | <nobr>完了</nobr> |
| 51 | xlsx: 塗りつぶし色とパターン | <nobr>完了</nobr> |
| 52 | xlsx: セルスタイルの適用 | <nobr>完了</nobr> |
| 53 | xlsx: セルスタイルの作成 | <nobr>完了</nobr> |
| 54 | xlsx: テーマ色・フォント・効果 | <nobr>完了</nobr> |
| 55 | xlsx: 書式クリア | <nobr>完了</nobr> |
| 56 | xlsx: 標準・数値形式 | <nobr>完了</nobr> |
| 57 | xlsx: 通貨・会計形式 | <nobr>完了</nobr> |
| 58 | xlsx: 割合・指数形式 | <nobr>完了</nobr> |
| 59 | xlsx: 分数形式 | <nobr>完了</nobr> |
| 60 | xlsx: 日付・時刻形式 | <nobr>完了</nobr> |
| 61 | xlsx: 文字列形式 | <nobr>完了</nobr> |
| 62 | xlsx: 正・負・ゼロ・文字の4区分 | <nobr>完了</nobr> |
| 63 | xlsx: 条件付き表示区分 | <nobr>完了</nobr> |
| 64 | xlsx: 表示形式の色指定 | <nobr>完了</nobr> |
| 65 | xlsx: 0・#・?の桁指定 | <nobr>完了</nobr> |
| 66 | xlsx: 小数点と桁区切り | <nobr>完了</nobr> |
| 67 | xlsx: 千・百万単位の表示 | <nobr>完了</nobr> |
| 68 | xlsx: 引用符とエスケープ文字 | <nobr>完了</nobr> |
| 69 | xlsx: 空白幅と文字反復 | <nobr>完了</nobr> |
| 70 | xlsx: 文字プレースホルダー | <nobr>完了</nobr> |
| 71 | xlsx: 経過時間の角括弧 | <nobr>完了</nobr> |
| 72 | xlsx: 月と分の文脈解釈 | <nobr>完了</nobr> |
| 73 | xlsx: ロケール・通貨記号 | <nobr>一部</nobr> |
| 74 | xlsx: 和暦表示と元号 | <nobr>完了</nobr> |
| 75 | xlsx: 表示値と保存数値の分離 | <nobr>完了</nobr> |
| 76 | xlsx: 値の大小・範囲・等値ルール | <nobr>完了</nobr> |
| 77 | xlsx: 文字列を含むルール | <nobr>完了</nobr> |
| 78 | xlsx: 日付の条件 | <nobr>完了</nobr> |
| 79 | xlsx: 重複・一意値 | <nobr>完了</nobr> |
| 80 | xlsx: 上位・下位の件数と割合 | <nobr>完了</nobr> |
| 81 | xlsx: 平均より上・下 | <nobr>完了</nobr> |
| 82 | xlsx: 数式によるルール | <nobr>完了</nobr> |
| 83 | xlsx: 2色・3色カラースケール | <nobr>完了</nobr> |
| 84 | xlsx: データバー | <nobr>完了</nobr> |
| 85 | xlsx: アイコンセット | <nobr>完了</nobr> |
| 86 | xlsx: しきい値の数値・割合・数式 | <nobr>完了</nobr> |
| 87 | xlsx: 負のデータバーと軸 | <nobr>完了</nobr> |
| 88 | xlsx: アイコンの逆順と個別選択 | <nobr>完了</nobr> |
| 89 | xlsx: ルールの適用範囲 | <nobr>完了</nobr> |
| 90 | xlsx: ルールの優先順位 | <nobr>完了</nobr> |
| 91 | xlsx: 条件を満たしたら停止 | <nobr>完了</nobr> |
| 92 | xlsx: ルールのコピー・編集・削除 | <nobr>完了</nobr> |
| 93 | xlsx: 四則・比較・文字結合演算子 | <nobr>完了</nobr> |
| 94 | xlsx: 演算子優先順位 | <nobr>完了</nobr> |
| 95 | xlsx: 相対・絶対・複合参照 | <nobr>完了</nobr> |
| 96 | xlsx: シート間参照 | <nobr>完了</nobr> |
| 97 | xlsx: 3D参照 | <nobr>完了</nobr> |
| 98 | xlsx: 外部ブック参照 | <nobr>一部</nobr> |
| 99 | xlsx: 定義名とスコープ | <nobr>完了</nobr> |
| 100 | xlsx: 名前付き数式 | <nobr>完了</nobr> |
| 101 | xlsx: 構造化参照 | <nobr>完了</nobr> |
| 102 | xlsx: 配列定数 | <nobr>完了</nobr> |
| 103 | xlsx: 従来の配列数式 | <nobr>完了</nobr> |
| 104 | xlsx: 動的配列とスピル | <nobr>完了</nobr> |
| 105 | xlsx: 暗黙の交差演算子 | <nobr>完了</nobr> |
| 106 | xlsx: スピル範囲演算子 | <nobr>完了</nobr> |
| 107 | xlsx: 共有数式 | <nobr>完了</nobr> |
| 108 | xlsx: 自動・手動・データテーブル以外の計算 | <nobr>完了</nobr> |
| 109 | xlsx: 反復計算と循環参照 | <nobr>完了</nobr> |
| 110 | xlsx: 1900・1904日付方式 | <nobr>完了</nobr> |
| 111 | xlsx: 表示桁数で計算 | <nobr>完了</nobr> |
| 112 | xlsx: 計算済みキャッシュ | <nobr>完了</nobr> |
| 113 | xlsx: エラー値とエラー伝播 | <nobr>完了</nobr> |
| 114 | xlsx: 参照元・参照先トレース | <nobr>完了</nobr> |
| 115 | xlsx: 数式検証とウォッチ | <nobr>完了</nobr> |
| 116 | xlsx: 数式オートコンプリート | <nobr>完了</nobr> |
| 117 | xlsx: 数学・三角関数 | <nobr>完了</nobr> |
| 118 | xlsx: 統計関数 | <nobr>一部</nobr> |
| 119 | xlsx: 財務関数 | <nobr>一部</nobr> |
| 120 | xlsx: 日付・時刻関数 | <nobr>完了</nobr> |
| 121 | xlsx: 検索・参照関数 | <nobr>一部</nobr> |
| 122 | xlsx: 文字列関数 | <nobr>一部</nobr> |
| 123 | xlsx: 論理関数 | <nobr>完了</nobr> |
| 124 | xlsx: 情報関数 | <nobr>完了</nobr> |
| 125 | xlsx: データベース関数 | <nobr>完了</nobr> |
| 126 | xlsx: エンジニアリング関数 | <nobr>完了</nobr> |
| 127 | xlsx: 互換性関数 | <nobr>一部</nobr> |
| 128 | xlsx: テーブル作成と範囲への変換 | <nobr>完了</nobr> |
| 129 | xlsx: テーブル名 | <nobr>完了</nobr> |
| 130 | xlsx: 見出し行 | <nobr>完了</nobr> |
| 131 | xlsx: 集計行 | <nobr>完了</nobr> |
| 132 | xlsx: 計算列の自動展開 | <nobr>完了</nobr> |
| 133 | xlsx: 行列追加時の範囲拡張 | <nobr>完了</nobr> |
| 134 | xlsx: テーブルスタイルと縞模様 | <nobr>完了</nobr> |
| 135 | xlsx: テーブルの構造化参照 | <nobr>完了</nobr> |
| 136 | xlsx: フィルターの値一覧選択 | <nobr>完了</nobr> |
| 137 | xlsx: 数値・日付・文字列フィルター | <nobr>完了</nobr> |
| 138 | xlsx: 色・アイコンによる抽出 | <nobr>完了</nobr> |
| 139 | xlsx: 複数キーの並べ替え | <nobr>完了</nobr> |
| 140 | xlsx: ユーザー定義リスト順 | <nobr>完了</nobr> |
| 141 | xlsx: 大文字小文字を区別する並べ替え | <nobr>完了</nobr> |
| 142 | xlsx: 詳細設定フィルター | <nobr>完了</nobr> |
| 143 | xlsx: 重複除去 | <nobr>完了</nobr> |
| 144 | xlsx: 区切り位置で列分割 | <nobr>完了</nobr> |
| 145 | xlsx: 統合による集計 | <nobr>完了</nobr> |
| 146 | xlsx: 整数・小数の入力制限 | <nobr>完了</nobr> |
| 147 | xlsx: リスト選択 | <nobr>完了</nobr> |
| 148 | xlsx: 日付・時刻の制限 | <nobr>完了</nobr> |
| 149 | xlsx: 文字列長の制限 | <nobr>完了</nobr> |
| 150 | xlsx: 数式による入力制限 | <nobr>完了</nobr> |
| 151 | xlsx: 空白の扱い | <nobr>完了</nobr> |
| 152 | xlsx: 入力時メッセージ | <nobr>完了</nobr> |
| 153 | xlsx: 停止・注意・情報のエラー表示 | <nobr>完了</nobr> |
| 154 | xlsx: 無効データのマーク | <nobr>完了</nobr> |
| 155 | xlsx: 入力規則のコピー | <nobr>完了</nobr> |
| 156 | xlsx: 入力規則の変更と削除 | <nobr>完了</nobr> |
| 157 | xlsx: 貼付・再計算で規則を回避する場合の評価 | <nobr>完了</nobr> |
| 158 | xlsx: 集合縦棒 | <nobr>完了</nobr> |
| 159 | xlsx: 積上げ縦棒・100%積上げ縦棒 | <nobr>完了</nobr> |
| 160 | xlsx: 3D縦棒と系列方向 | <nobr>一部</nobr> |
| 161 | xlsx: 集合横棒 | <nobr>完了</nobr> |
| 162 | xlsx: 積上げ横棒・100%積上げ横棒 | <nobr>完了</nobr> |
| 163 | xlsx: 3D横棒 | <nobr>一部</nobr> |
| 164 | xlsx: 折れ線とマーカー | <nobr>完了</nobr> |
| 165 | xlsx: 積上げ折れ線・100%積上げ折れ線 | <nobr>完了</nobr> |
| 166 | xlsx: 3D折れ線 | <nobr>一部</nobr> |
| 167 | xlsx: 円グラフ・3D円グラフ | <nobr>一部</nobr> |
| 168 | xlsx: 補助円・補助棒付き円 | <nobr>完了</nobr> |
| 169 | xlsx: ドーナツ | <nobr>完了</nobr> |
| 170 | xlsx: 面グラフ | <nobr>完了</nobr> |
| 171 | xlsx: 積上げ面・100%積上げ面 | <nobr>完了</nobr> |
| 172 | xlsx: 3D面 | <nobr>一部</nobr> |
| 173 | xlsx: 散布図の点・直線・平滑線 | <nobr>完了</nobr> |
| 174 | xlsx: バブルと3D効果 | <nobr>一部</nobr> |
| 175 | xlsx: 株価の高値安値終値 | <nobr>完了</nobr> |
| 176 | xlsx: 株価の始値高値安値終値 | <nobr>完了</nobr> |
| 177 | xlsx: 株価の出来高付き型 | <nobr>完了</nobr> |
| 178 | xlsx: レーダー・マーカー・塗りつぶし | <nobr>完了</nobr> |
| 179 | xlsx: 組合せと第2軸 | <nobr>完了</nobr> |
| 180 | xlsx: データ範囲と系列の変更 | <nobr>完了</nobr> |
| 181 | xlsx: 行列の切替 | <nobr>完了</nobr> |
| 182 | xlsx: タイトルと軸ラベル | <nobr>完了</nobr> |
| 183 | xlsx: 凡例の位置 | <nobr>完了</nobr> |
| 184 | xlsx: データラベルとセル参照 | <nobr>完了</nobr> |
| 185 | xlsx: データテーブル | <nobr>完了</nobr> |
| 186 | xlsx: 軸の最小値・最大値・単位 | <nobr>完了</nobr> |
| 187 | xlsx: 軸の対数目盛 | <nobr>完了</nobr> |
| 188 | xlsx: 日付軸とテキスト軸 | <nobr>完了</nobr> |
| 189 | xlsx: 軸の交点と逆順 | <nobr>完了</nobr> |
| 190 | xlsx: 第2軸と系列軸 | <nobr>完了</nobr> |
| 191 | xlsx: 目盛線 | <nobr>完了</nobr> |
| 192 | xlsx: 系列の重なりと間隔 | <nobr>完了</nobr> |
| 193 | xlsx: マーカーと線書式 | <nobr>完了</nobr> |
| 194 | xlsx: 塗り・輪郭・効果 | <nobr>完了</nobr> |
| 195 | xlsx: 誤差範囲 | <nobr>完了</nobr> |
| 196 | xlsx: 近似曲線と式・決定係数 | <nobr>完了</nobr> |
| 197 | xlsx: 空白・非表示データの表示 | <nobr>完了</nobr> |
| 198 | xlsx: グラフスタイルと配色 | <nobr>完了</nobr> |
| 199 | xlsx: グラフシートへ移動 | <nobr>完了</nobr> |
| 200 | xlsx: スパークラインと軸 | <nobr>完了</nobr> |
| 201 | xlsx: 図形・画像・SVG・SmartArt | <nobr>一部</nobr> |
| 202 | xlsx: ピボットの作成と元範囲 | <nobr>完了</nobr> |
| 203 | xlsx: 行・列・値・フィルターの配置 | <nobr>完了</nobr> |
| 204 | xlsx: 合計・個数・平均等の集計 | <nobr>完了</nobr> |
| 205 | xlsx: 値の表示方法と比率 | <nobr>完了</nobr> |
| 206 | xlsx: 小計と総計 | <nobr>完了</nobr> |
| 207 | xlsx: コンパクト・アウトライン・表形式 | <nobr>完了</nobr> |
| 208 | xlsx: 項目ラベルの繰返し | <nobr>完了</nobr> |
| 209 | xlsx: 空白とエラー値表示 | <nobr>完了</nobr> |
| 210 | xlsx: 日付・数値のグループ化 | <nobr>完了</nobr> |
| 211 | xlsx: 手動グループ化 | <nobr>完了</nobr> |
| 212 | xlsx: 計算フィールド | <nobr>完了</nobr> |
| 213 | xlsx: 計算アイテム | <nobr>完了</nobr> |
| 214 | xlsx: 項目の並べ替えと抽出 | <nobr>完了</nobr> |
| 215 | xlsx: 上位項目と値フィルター | <nobr>完了</nobr> |
| 216 | xlsx: 更新とキャッシュ | <nobr>完了</nobr> |
| 217 | xlsx: 詳細データの展開 | <nobr>完了</nobr> |
| 218 | xlsx: ピボット書式の維持 | <nobr>完了</nobr> |
| 219 | xlsx: CSV・テキスト取込 | <nobr>一部</nobr> |
| 220 | xlsx: モデル保存と未対応環境での表示 | <nobr>一部</nobr> |
| 221 | xlsx: ゴールシークの目標と変数 | <nobr>完了</nobr> |
| 222 | xlsx: 1変数データテーブル | <nobr>完了</nobr> |
| 223 | xlsx: 2変数データテーブル | <nobr>完了</nobr> |
| 224 | xlsx: シナリオの作成 | <nobr>完了</nobr> |
| 225 | xlsx: シナリオ切替と編集 | <nobr>完了</nobr> |
| 226 | xlsx: シナリオの統合 | <nobr>完了</nobr> |
| 227 | xlsx: シナリオの要約出力 | <nobr>完了</nobr> |
| 228 | xlsx: 線形・指数傾向のフィル | <nobr>完了</nobr> |
| 229 | xlsx: 相関分析 | <nobr>一部</nobr> |
| 230 | xlsx: 共分散分析 | <nobr>一部</nobr> |
| 231 | xlsx: 基本統計量 | <nobr>一部</nobr> |
| 232 | xlsx: F検定 | <nobr>一部</nobr> |
| 233 | xlsx: ヒストグラム | <nobr>一部</nobr> |
| 234 | xlsx: 乱数生成 | <nobr>一部</nobr> |
| 235 | xlsx: 順位と百分位 | <nobr>一部</nobr> |
| 236 | xlsx: 回帰分析 | <nobr>一部</nobr> |
| 237 | xlsx: 対応ありt検定 | <nobr>一部</nobr> |
| 238 | xlsx: 等分散・非等分散t検定 | <nobr>一部</nobr> |
| 239 | xlsx: z検定 | <nobr>一部</nobr> |
| 240 | xlsx: 予測シート | <nobr>一部</nobr> |
| 241 | xlsx: セルのロックと数式非表示 | <nobr>完了</nobr> |
| 242 | xlsx: シート保護とパスワード | <nobr>完了</nobr> |
| 243 | xlsx: 保護中の許可操作 | <nobr>完了</nobr> |
| 244 | xlsx: 編集可能な範囲 | <nobr>完了</nobr> |
| 245 | xlsx: ブック構成の保護 | <nobr>完了</nobr> |
| 246 | xlsx: コメントスレッド | <nobr>完了</nobr> |
| 247 | xlsx: 従来のメモ | <nobr>完了</nobr> |
| 248 | xlsx: 印刷範囲 | <nobr>完了</nobr> |
| 249 | xlsx: 改ページ | <nobr>完了</nobr> |
| 250 | xlsx: 用紙サイズ・方向・余白 | <nobr>完了</nobr> |
| 251 | xlsx: 印刷倍率とページ数調整 | <nobr>完了</nobr> |
| 252 | xlsx: 印刷タイトル行列 | <nobr>完了</nobr> |
| 253 | xlsx: ヘッダー・フッターのコード | <nobr>完了</nobr> |
| 254 | xlsx: グリッド線・見出し印刷 | <nobr>完了</nobr> |
| 255 | xlsx: ページ順と中央配置 | <nobr>完了</nobr> |
| 256 | xlsx: 選択範囲・シート・ブック印刷 | <nobr>完了</nobr> |
| 257 | xlsx: PDF・CSV・旧xls出力 | <nobr>完了</nobr> |
| 258 | xlsx: アクセシビリティ検査 | <nobr>完了</nobr> |
| 259 | xlsx: 画像等の代替テキスト | <nobr>完了</nobr> |
| 260 | xlsx: 公式アルファベット一覧と個別関数IDの差分を確認 | <nobr>一部</nobr> |
| 261 | xlsx: カテゴリだけでは個別関数の追跡漏れが起こるため名前単位で照合 | <nobr>一部</nobr> |
| 262 | xlsx: 既存526関数の試験一覧と公式収録521名の差を照合 | <nobr>一部</nobr> |
| 263 | xlsx: 各関数の引数・省略・配列・エラー・精度・版差を子項目へ展開 | <nobr>一部</nobr> |
| 264 | xlsx: 表示形式・条件付き書式・グラフ下位型の全enum値を照合 | <nobr>一部</nobr> |
| 265 | xlsx: Windows／Mac／Webの表示・編集・保存・計算・更新差を記録 | <nobr>一部</nobr> |
| 266 | xlsx: OOXML標準・拡張と既存coverageを対応 | <nobr>一部</nobr> |
| 267 | xlsx: ABS | <nobr>完了</nobr> |
| 268 | xlsx: ACCRINT | <nobr>一部</nobr> |
| 269 | xlsx: ACCRINTM | <nobr>完了</nobr> |
| 270 | xlsx: ACOS | <nobr>完了</nobr> |
| 271 | xlsx: ACOSH | <nobr>完了</nobr> |
| 272 | xlsx: ACOT | <nobr>完了</nobr> |
| 273 | xlsx: ACOTH | <nobr>完了</nobr> |
| 274 | xlsx: ADDRESS | <nobr>完了</nobr> |
| 275 | xlsx: AGGREGATE | <nobr>完了</nobr> |
| 276 | xlsx: AMORDEGRC | <nobr>一部</nobr> |
| 277 | xlsx: AMORLINC | <nobr>完了</nobr> |
| 278 | xlsx: AND | <nobr>完了</nobr> |
| 279 | xlsx: ARABIC | <nobr>完了</nobr> |
| 280 | xlsx: AREAS | <nobr>完了</nobr> |
| 281 | xlsx: ARRAYTOTEXT | <nobr>完了</nobr> |
| 282 | xlsx: ASC | <nobr>完了</nobr> |
| 283 | xlsx: ASIN | <nobr>完了</nobr> |
| 284 | xlsx: ASINH | <nobr>完了</nobr> |
| 285 | xlsx: ATAN | <nobr>完了</nobr> |
| 286 | xlsx: ATAN2 | <nobr>完了</nobr> |
| 287 | xlsx: ATANH | <nobr>完了</nobr> |
| 288 | xlsx: AVEDEV | <nobr>完了</nobr> |
| 289 | xlsx: AVERAGE | <nobr>完了</nobr> |
| 290 | xlsx: AVERAGEA | <nobr>完了</nobr> |
| 291 | xlsx: AVERAGEIF | <nobr>完了</nobr> |
| 292 | xlsx: AVERAGEIFS | <nobr>完了</nobr> |
| 293 | xlsx: BASE | <nobr>完了</nobr> |
| 294 | xlsx: BESSELI | <nobr>一部</nobr> |
| 295 | xlsx: BESSELJ | <nobr>一部</nobr> |
| 296 | xlsx: BESSELK | <nobr>一部</nobr> |
| 297 | xlsx: BESSELY | <nobr>一部</nobr> |
| 298 | xlsx: BETA.DIST | <nobr>一部</nobr> |
| 299 | xlsx: BETA.INV | <nobr>一部</nobr> |
| 300 | xlsx: BETADIST | <nobr>一部</nobr> |
| 301 | xlsx: BETAINV | <nobr>一部</nobr> |
| 302 | xlsx: BIN2DEC | <nobr>完了</nobr> |
| 303 | xlsx: BIN2HEX | <nobr>完了</nobr> |
| 304 | xlsx: BIN2OCT | <nobr>完了</nobr> |
| 305 | xlsx: BINOM.DIST | <nobr>完了</nobr> |
| 306 | xlsx: BINOM.DIST.RANGE | <nobr>完了</nobr> |
| 307 | xlsx: BINOM.INV | <nobr>一部</nobr> |
| 308 | xlsx: BINOMDIST | <nobr>完了</nobr> |
| 309 | xlsx: BITAND | <nobr>完了</nobr> |
| 310 | xlsx: BITLSHIFT | <nobr>完了</nobr> |
| 311 | xlsx: BITOR | <nobr>完了</nobr> |
| 312 | xlsx: BITRSHIFT | <nobr>完了</nobr> |
| 313 | xlsx: BITXOR | <nobr>完了</nobr> |
| 314 | xlsx: BYCOL | <nobr>完了</nobr> |
| 315 | xlsx: BYROW | <nobr>完了</nobr> |
| 316 | xlsx: CEILING | <nobr>完了</nobr> |
| 317 | xlsx: CEILING.MATH | <nobr>完了</nobr> |
| 318 | xlsx: CEILING.PRECISE | <nobr>完了</nobr> |
| 319 | xlsx: CELL | <nobr>一部</nobr> |
| 320 | xlsx: CHAR | <nobr>完了</nobr> |
| 321 | xlsx: CHIDIST | <nobr>完了</nobr> |
| 322 | xlsx: CHIINV | <nobr>一部</nobr> |
| 323 | xlsx: CHISQ.DIST | <nobr>一部</nobr> |
| 324 | xlsx: CHISQ.DIST.RT | <nobr>完了</nobr> |
| 325 | xlsx: CHISQ.INV | <nobr>完了</nobr> |
| 326 | xlsx: CHISQ.INV.RT | <nobr>完了</nobr> |
| 327 | xlsx: CHISQ.TEST | <nobr>一部</nobr> |
| 328 | xlsx: CHITEST | <nobr>一部</nobr> |
| 329 | xlsx: CHOOSE | <nobr>完了</nobr> |
| 330 | xlsx: CHOOSECOLS | <nobr>完了</nobr> |
| 331 | xlsx: CHOOSEROWS | <nobr>完了</nobr> |
| 332 | xlsx: CLEAN | <nobr>完了</nobr> |
| 333 | xlsx: CODE | <nobr>完了</nobr> |
| 334 | xlsx: COLUMN | <nobr>完了</nobr> |
| 335 | xlsx: COLUMNS | <nobr>完了</nobr> |
| 336 | xlsx: COMBIN | <nobr>完了</nobr> |
| 337 | xlsx: COMBINA | <nobr>完了</nobr> |
| 338 | xlsx: COMPLEX | <nobr>完了</nobr> |
| 339 | xlsx: CONCAT | <nobr>完了</nobr> |
| 340 | xlsx: CONCATENATE | <nobr>完了</nobr> |
| 341 | xlsx: CONFIDENCE | <nobr>完了</nobr> |
| 342 | xlsx: CONFIDENCE.NORM | <nobr>一部</nobr> |
| 343 | xlsx: CONFIDENCE.T | <nobr>一部</nobr> |
| 344 | xlsx: CONVERT | <nobr>完了</nobr> |
| 345 | xlsx: CORREL | <nobr>完了</nobr> |
| 346 | xlsx: COS | <nobr>完了</nobr> |
| 347 | xlsx: COSH | <nobr>完了</nobr> |
| 348 | xlsx: COT | <nobr>完了</nobr> |
| 349 | xlsx: COTH | <nobr>完了</nobr> |
| 350 | xlsx: COUNT | <nobr>完了</nobr> |
| 351 | xlsx: COUNTA | <nobr>完了</nobr> |
| 352 | xlsx: COUNTBLANK | <nobr>完了</nobr> |
| 353 | xlsx: COUNTIF | <nobr>完了</nobr> |
| 354 | xlsx: COUNTIFS | <nobr>完了</nobr> |
| 355 | xlsx: COUPDAYBS | <nobr>完了</nobr> |
| 356 | xlsx: COUPDAYS | <nobr>完了</nobr> |
| 357 | xlsx: COUPDAYSNC | <nobr>完了</nobr> |
| 358 | xlsx: COUPNCD | <nobr>完了</nobr> |
| 359 | xlsx: COUPNUM | <nobr>完了</nobr> |
| 360 | xlsx: COUPPCD | <nobr>完了</nobr> |
| 361 | xlsx: COVAR | <nobr>完了</nobr> |
| 362 | xlsx: COVARIANCE.P | <nobr>完了</nobr> |
| 363 | xlsx: COVARIANCE.S | <nobr>完了</nobr> |
| 364 | xlsx: CRITBINOM | <nobr>一部</nobr> |
| 365 | xlsx: CSC | <nobr>完了</nobr> |
| 366 | xlsx: CSCH | <nobr>完了</nobr> |
| 367 | xlsx: CUMIPMT | <nobr>一部</nobr> |
| 368 | xlsx: CUMPRINC | <nobr>一部</nobr> |
| 369 | xlsx: DATE | <nobr>完了</nobr> |
| 370 | xlsx: DATEDIF | <nobr>完了</nobr> |
| 371 | xlsx: DATEVALUE | <nobr>一部</nobr> |
| 372 | xlsx: DAVERAGE | <nobr>完了</nobr> |
| 373 | xlsx: DAY | <nobr>完了</nobr> |
| 374 | xlsx: DAYS | <nobr>完了</nobr> |
| 375 | xlsx: DAYS360 | <nobr>完了</nobr> |
| 376 | xlsx: DB | <nobr>完了</nobr> |
| 377 | xlsx: DBCS | <nobr>完了</nobr> |
| 378 | xlsx: DCOUNT | <nobr>完了</nobr> |
| 379 | xlsx: DCOUNTA | <nobr>完了</nobr> |
| 380 | xlsx: DDB | <nobr>完了</nobr> |
| 381 | xlsx: DEC2BIN | <nobr>完了</nobr> |
| 382 | xlsx: DEC2HEX | <nobr>完了</nobr> |
| 383 | xlsx: DEC2OCT | <nobr>完了</nobr> |
| 384 | xlsx: DECIMAL | <nobr>完了</nobr> |
| 385 | xlsx: DEGREES | <nobr>完了</nobr> |
| 386 | xlsx: DELTA | <nobr>完了</nobr> |
| 387 | xlsx: DEVSQ | <nobr>完了</nobr> |
| 388 | xlsx: DGET | <nobr>完了</nobr> |
| 389 | xlsx: DISC | <nobr>完了</nobr> |
| 390 | xlsx: DMAX | <nobr>完了</nobr> |
| 391 | xlsx: DMIN | <nobr>完了</nobr> |
| 392 | xlsx: DOLLAR | <nobr>完了</nobr> |
| 393 | xlsx: DOLLARDE | <nobr>完了</nobr> |
| 394 | xlsx: DOLLARFR | <nobr>完了</nobr> |
| 395 | xlsx: DPRODUCT | <nobr>完了</nobr> |
| 396 | xlsx: DROP | <nobr>完了</nobr> |
| 397 | xlsx: DSTDEV | <nobr>完了</nobr> |
| 398 | xlsx: DSTDEVP | <nobr>完了</nobr> |
| 399 | xlsx: DSUM | <nobr>完了</nobr> |
| 400 | xlsx: DURATION | <nobr>完了</nobr> |
| 401 | xlsx: DVAR | <nobr>完了</nobr> |
| 402 | xlsx: DVARP | <nobr>完了</nobr> |
| 403 | xlsx: EDATE | <nobr>完了</nobr> |
| 404 | xlsx: EFFECT | <nobr>完了</nobr> |
| 405 | xlsx: ENCODEURL | <nobr>完了</nobr> |
| 406 | xlsx: EOMONTH | <nobr>完了</nobr> |
| 407 | xlsx: ERF | <nobr>完了</nobr> |
| 408 | xlsx: ERF.PRECISE | <nobr>完了</nobr> |
| 409 | xlsx: ERFC | <nobr>完了</nobr> |
| 410 | xlsx: ERFC.PRECISE | <nobr>完了</nobr> |
| 411 | xlsx: ERROR.TYPE | <nobr>完了</nobr> |
| 412 | xlsx: EVEN | <nobr>完了</nobr> |
| 413 | xlsx: EXACT | <nobr>完了</nobr> |
| 414 | xlsx: EXP | <nobr>完了</nobr> |
| 415 | xlsx: EXPAND | <nobr>完了</nobr> |
| 416 | xlsx: EXPON.DIST | <nobr>完了</nobr> |
| 417 | xlsx: EXPONDIST | <nobr>完了</nobr> |
| 418 | xlsx: F.DIST | <nobr>一部</nobr> |
| 419 | xlsx: F.DIST.RT | <nobr>一部</nobr> |
| 420 | xlsx: F.INV | <nobr>一部</nobr> |
| 421 | xlsx: F.INV.RT | <nobr>一部</nobr> |
| 422 | xlsx: F.TEST | <nobr>一部</nobr> |
| 423 | xlsx: FACT | <nobr>完了</nobr> |
| 424 | xlsx: FACTDOUBLE | <nobr>完了</nobr> |
| 425 | xlsx: FALSE | <nobr>完了</nobr> |
| 426 | xlsx: FDIST | <nobr>一部</nobr> |
| 427 | xlsx: FILTER | <nobr>完了</nobr> |
| 428 | xlsx: FILTERXML | <nobr>完了</nobr> |
| 429 | xlsx: FIND | <nobr>完了</nobr> |
| 430 | xlsx: FINDB | <nobr>完了</nobr> |
| 431 | xlsx: FINV | <nobr>一部</nobr> |
| 432 | xlsx: FISHER | <nobr>完了</nobr> |
| 433 | xlsx: FISHERINV | <nobr>完了</nobr> |
| 434 | xlsx: FIXED | <nobr>完了</nobr> |
| 435 | xlsx: FLOOR | <nobr>完了</nobr> |
| 436 | xlsx: FLOOR.MATH | <nobr>完了</nobr> |
| 437 | xlsx: FLOOR.PRECISE | <nobr>完了</nobr> |
| 438 | xlsx: FORECAST | <nobr>完了</nobr> |
| 439 | xlsx: FORECAST.LINEAR | <nobr>完了</nobr> |
| 440 | xlsx: FORMULATEXT | <nobr>完了</nobr> |
| 441 | xlsx: FREQUENCY | <nobr>一部</nobr> |
| 442 | xlsx: FTEST | <nobr>一部</nobr> |
| 443 | xlsx: FV | <nobr>完了</nobr> |
| 444 | xlsx: FVSCHEDULE | <nobr>完了</nobr> |
| 445 | xlsx: GAMMA | <nobr>一部</nobr> |
| 446 | xlsx: GAMMA.DIST | <nobr>一部</nobr> |
| 447 | xlsx: GAMMA.INV | <nobr>一部</nobr> |
| 448 | xlsx: GAMMADIST | <nobr>完了</nobr> |
| 449 | xlsx: GAMMAINV | <nobr>完了</nobr> |
| 450 | xlsx: GAMMALN | <nobr>完了</nobr> |
| 451 | xlsx: GAMMALN.PRECISE | <nobr>完了</nobr> |
| 452 | xlsx: GAUSS | <nobr>完了</nobr> |
| 453 | xlsx: GCD | <nobr>完了</nobr> |
| 454 | xlsx: GEOMEAN | <nobr>完了</nobr> |
| 455 | xlsx: GESTEP | <nobr>完了</nobr> |
| 456 | xlsx: GROWTH | <nobr>一部</nobr> |
| 457 | xlsx: HARMEAN | <nobr>完了</nobr> |
| 458 | xlsx: HEX2BIN | <nobr>完了</nobr> |
| 459 | xlsx: HEX2DEC | <nobr>完了</nobr> |
| 460 | xlsx: HEX2OCT | <nobr>完了</nobr> |
| 461 | xlsx: HLOOKUP | <nobr>完了</nobr> |
| 462 | xlsx: HOUR | <nobr>完了</nobr> |
| 463 | xlsx: HSTACK | <nobr>完了</nobr> |
| 464 | xlsx: HYPERLINK | <nobr>完了</nobr> |
| 465 | xlsx: HYPGEOM.DIST | <nobr>一部</nobr> |
| 466 | xlsx: HYPGEOMDIST | <nobr>一部</nobr> |
| 467 | xlsx: IF | <nobr>完了</nobr> |
| 468 | xlsx: IFERROR | <nobr>完了</nobr> |
| 469 | xlsx: IFNA | <nobr>完了</nobr> |
| 470 | xlsx: IFS | <nobr>完了</nobr> |
| 471 | xlsx: IMABS | <nobr>完了</nobr> |
| 472 | xlsx: IMAGE | <nobr>完了</nobr> |
| 473 | xlsx: IMAGINARY | <nobr>完了</nobr> |
| 474 | xlsx: IMARGUMENT | <nobr>完了</nobr> |
| 475 | xlsx: IMCONJUGATE | <nobr>完了</nobr> |
| 476 | xlsx: IMCOS | <nobr>完了</nobr> |
| 477 | xlsx: IMCOSH | <nobr>完了</nobr> |
| 478 | xlsx: IMCOT | <nobr>完了</nobr> |
| 479 | xlsx: IMCSC | <nobr>完了</nobr> |
| 480 | xlsx: IMCSCH | <nobr>完了</nobr> |
| 481 | xlsx: IMDIV | <nobr>完了</nobr> |
| 482 | xlsx: IMEXP | <nobr>完了</nobr> |
| 483 | xlsx: IMLN | <nobr>完了</nobr> |
| 484 | xlsx: IMLOG10 | <nobr>完了</nobr> |
| 485 | xlsx: IMLOG2 | <nobr>完了</nobr> |
| 486 | xlsx: IMPOWER | <nobr>一部</nobr> |
| 487 | xlsx: IMPRODUCT | <nobr>完了</nobr> |
| 488 | xlsx: IMREAL | <nobr>完了</nobr> |
| 489 | xlsx: IMSEC | <nobr>完了</nobr> |
| 490 | xlsx: IMSECH | <nobr>完了</nobr> |
| 491 | xlsx: IMSIN | <nobr>完了</nobr> |
| 492 | xlsx: IMSINH | <nobr>完了</nobr> |
| 493 | xlsx: IMSQRT | <nobr>完了</nobr> |
| 494 | xlsx: IMSUB | <nobr>完了</nobr> |
| 495 | xlsx: IMSUM | <nobr>完了</nobr> |
| 496 | xlsx: IMTAN | <nobr>完了</nobr> |
| 497 | xlsx: INDEX | <nobr>完了</nobr> |
| 498 | xlsx: INDIRECT | <nobr>完了</nobr> |
| 499 | xlsx: INFO | <nobr>一部</nobr> |
| 500 | xlsx: INT | <nobr>完了</nobr> |
| 501 | xlsx: INTERCEPT | <nobr>完了</nobr> |
| 502 | xlsx: INTRATE | <nobr>完了</nobr> |
| 503 | xlsx: IPMT | <nobr>完了</nobr> |
| 504 | xlsx: IRR | <nobr>一部</nobr> |
| 505 | xlsx: ISBLANK | <nobr>完了</nobr> |
| 506 | xlsx: ISERR | <nobr>完了</nobr> |
| 507 | xlsx: ISERROR | <nobr>完了</nobr> |
| 508 | xlsx: ISEVEN | <nobr>完了</nobr> |
| 509 | xlsx: ISFORMULA | <nobr>完了</nobr> |
| 510 | xlsx: ISLOGICAL | <nobr>完了</nobr> |
| 511 | xlsx: ISNA | <nobr>完了</nobr> |
| 512 | xlsx: ISNONTEXT | <nobr>完了</nobr> |
| 513 | xlsx: ISNUMBER | <nobr>完了</nobr> |
| 514 | xlsx: ISO.CEILING | <nobr>完了</nobr> |
| 515 | xlsx: ISODD | <nobr>完了</nobr> |
| 516 | xlsx: ISOMITTED | <nobr>完了</nobr> |
| 517 | xlsx: ISOWEEKNUM | <nobr>完了</nobr> |
| 518 | xlsx: ISPMT | <nobr>完了</nobr> |
| 519 | xlsx: ISREF | <nobr>完了</nobr> |
| 520 | xlsx: ISTEXT | <nobr>完了</nobr> |
| 521 | xlsx: JIS | <nobr>一部</nobr> |
| 522 | xlsx: KURT | <nobr>完了</nobr> |
| 523 | xlsx: LAMBDA | <nobr>完了</nobr> |
| 524 | xlsx: LARGE | <nobr>完了</nobr> |
| 525 | xlsx: LCM | <nobr>完了</nobr> |
| 526 | xlsx: LEFT | <nobr>完了</nobr> |
| 527 | xlsx: LEFTB | <nobr>完了</nobr> |
| 528 | xlsx: LEN | <nobr>完了</nobr> |
| 529 | xlsx: LENB | <nobr>完了</nobr> |
| 530 | xlsx: LET | <nobr>完了</nobr> |
| 531 | xlsx: LINEST | <nobr>一部</nobr> |
| 532 | xlsx: LN | <nobr>完了</nobr> |
| 533 | xlsx: LOG | <nobr>完了</nobr> |
| 534 | xlsx: LOG10 | <nobr>完了</nobr> |
| 535 | xlsx: LOGEST | <nobr>一部</nobr> |
| 536 | xlsx: LOGINV | <nobr>完了</nobr> |
| 537 | xlsx: LOGNORM.DIST | <nobr>一部</nobr> |
| 538 | xlsx: LOGNORM.INV | <nobr>完了</nobr> |
| 539 | xlsx: LOGNORMDIST | <nobr>完了</nobr> |
| 540 | xlsx: LOOKUP | <nobr>完了</nobr> |
| 541 | xlsx: LOWER | <nobr>完了</nobr> |
| 542 | xlsx: MAKEARRAY | <nobr>一部</nobr> |
| 543 | xlsx: MAP | <nobr>完了</nobr> |
| 544 | xlsx: MATCH | <nobr>完了</nobr> |
| 545 | xlsx: MAX | <nobr>完了</nobr> |
| 546 | xlsx: MAXA | <nobr>完了</nobr> |
| 547 | xlsx: MAXIFS | <nobr>完了</nobr> |
| 548 | xlsx: MDETERM | <nobr>完了</nobr> |
| 549 | xlsx: MDURATION | <nobr>完了</nobr> |
| 550 | xlsx: MEDIAN | <nobr>完了</nobr> |
| 551 | xlsx: MID | <nobr>完了</nobr> |
| 552 | xlsx: MIDB | <nobr>完了</nobr> |
| 553 | xlsx: MIN | <nobr>完了</nobr> |
| 554 | xlsx: MINA | <nobr>完了</nobr> |
| 555 | xlsx: MINIFS | <nobr>完了</nobr> |
| 556 | xlsx: MINUTE | <nobr>完了</nobr> |
| 557 | xlsx: MINVERSE | <nobr>一部</nobr> |
| 558 | xlsx: MIRR | <nobr>完了</nobr> |
| 559 | xlsx: MMULT | <nobr>完了</nobr> |
| 560 | xlsx: MOD | <nobr>完了</nobr> |
| 561 | xlsx: MODE | <nobr>完了</nobr> |
| 562 | xlsx: MODE.MULT | <nobr>完了</nobr> |
| 563 | xlsx: MODE.SNGL | <nobr>完了</nobr> |
| 564 | xlsx: MONTH | <nobr>完了</nobr> |
| 565 | xlsx: MROUND | <nobr>完了</nobr> |
| 566 | xlsx: MULTINOMIAL | <nobr>一部</nobr> |
| 567 | xlsx: MUNIT | <nobr>完了</nobr> |
| 568 | xlsx: N | <nobr>完了</nobr> |
| 569 | xlsx: NA | <nobr>完了</nobr> |
| 570 | xlsx: NEGBINOM.DIST | <nobr>一部</nobr> |
| 571 | xlsx: NEGBINOMDIST | <nobr>一部</nobr> |
| 572 | xlsx: NETWORKDAYS | <nobr>完了</nobr> |
| 573 | xlsx: NETWORKDAYS.INTL | <nobr>完了</nobr> |
| 574 | xlsx: NOMINAL | <nobr>完了</nobr> |
| 575 | xlsx: NORM.DIST | <nobr>完了</nobr> |
| 576 | xlsx: NORM.INV | <nobr>完了</nobr> |
| 577 | xlsx: NORM.S.DIST | <nobr>完了</nobr> |
| 578 | xlsx: NORM.S.INV | <nobr>完了</nobr> |
| 579 | xlsx: NORMDIST | <nobr>完了</nobr> |
| 580 | xlsx: NORMINV | <nobr>完了</nobr> |
| 581 | xlsx: NORMSDIST | <nobr>完了</nobr> |
| 582 | xlsx: NORMSINV | <nobr>完了</nobr> |
| 583 | xlsx: NOT | <nobr>完了</nobr> |
| 584 | xlsx: NOW | <nobr>完了</nobr> |
| 585 | xlsx: NPER | <nobr>完了</nobr> |
| 586 | xlsx: NPV | <nobr>完了</nobr> |
| 587 | xlsx: NUMBERVALUE | <nobr>一部</nobr> |
| 588 | xlsx: OCT2BIN | <nobr>完了</nobr> |
| 589 | xlsx: OCT2DEC | <nobr>完了</nobr> |
| 590 | xlsx: OCT2HEX | <nobr>完了</nobr> |
| 591 | xlsx: ODD | <nobr>完了</nobr> |
| 592 | xlsx: ODDFPRICE | <nobr>一部</nobr> |
| 593 | xlsx: ODDFYIELD | <nobr>一部</nobr> |
| 594 | xlsx: ODDLPRICE | <nobr>完了</nobr> |
| 595 | xlsx: ODDLYIELD | <nobr>一部</nobr> |
| 596 | xlsx: OFFSET | <nobr>完了</nobr> |
| 597 | xlsx: OR | <nobr>完了</nobr> |
| 598 | xlsx: PDURATION | <nobr>完了</nobr> |
| 599 | xlsx: PEARSON | <nobr>完了</nobr> |
| 600 | xlsx: PERCENTILE | <nobr>完了</nobr> |
| 601 | xlsx: PERCENTILE.EXC | <nobr>完了</nobr> |
| 602 | xlsx: PERCENTILE.INC | <nobr>完了</nobr> |
| 603 | xlsx: PERCENTOF | <nobr>完了</nobr> |
| 604 | xlsx: PERCENTRANK | <nobr>完了</nobr> |
| 605 | xlsx: PERCENTRANK.EXC | <nobr>完了</nobr> |
| 606 | xlsx: PERCENTRANK.INC | <nobr>完了</nobr> |
| 607 | xlsx: PERMUT | <nobr>完了</nobr> |
| 608 | xlsx: PERMUTATIONA | <nobr>完了</nobr> |
| 609 | xlsx: PHI | <nobr>完了</nobr> |
| 610 | xlsx: PHONETIC | <nobr>完了</nobr> |
| 611 | xlsx: PI | <nobr>完了</nobr> |
| 612 | xlsx: PMT | <nobr>完了</nobr> |
| 613 | xlsx: POISSON | <nobr>一部</nobr> |
| 614 | xlsx: POISSON.DIST | <nobr>一部</nobr> |
| 615 | xlsx: POWER | <nobr>一部</nobr> |
| 616 | xlsx: PPMT | <nobr>一部</nobr> |
| 617 | xlsx: PRICE | <nobr>一部</nobr> |
| 618 | xlsx: PRICEDISC | <nobr>完了</nobr> |
| 619 | xlsx: PRICEMAT | <nobr>一部</nobr> |
| 620 | xlsx: PROB | <nobr>完了</nobr> |
| 621 | xlsx: PRODUCT | <nobr>完了</nobr> |
| 622 | xlsx: PROPER | <nobr>完了</nobr> |
| 623 | xlsx: PV | <nobr>完了</nobr> |
| 624 | xlsx: QUARTILE | <nobr>完了</nobr> |
| 625 | xlsx: QUARTILE.EXC | <nobr>完了</nobr> |
| 626 | xlsx: QUARTILE.INC | <nobr>完了</nobr> |
| 627 | xlsx: QUOTIENT | <nobr>完了</nobr> |
| 628 | xlsx: RADIANS | <nobr>完了</nobr> |
| 629 | xlsx: RAND | <nobr>一部</nobr> |
| 630 | xlsx: RANDARRAY | <nobr>一部</nobr> |
| 631 | xlsx: RANDBETWEEN | <nobr>一部</nobr> |
| 632 | xlsx: RANK | <nobr>完了</nobr> |
| 633 | xlsx: RANK.AVG | <nobr>完了</nobr> |
| 634 | xlsx: RANK.EQ | <nobr>完了</nobr> |
| 635 | xlsx: RATE | <nobr>一部</nobr> |
| 636 | xlsx: RECEIVED | <nobr>完了</nobr> |
| 637 | xlsx: REDUCE | <nobr>完了</nobr> |
| 638 | xlsx: REPLACE | <nobr>完了</nobr> |
| 639 | xlsx: REPLACEB | <nobr>完了</nobr> |
| 640 | xlsx: REPT | <nobr>完了</nobr> |
| 641 | xlsx: RIGHT | <nobr>完了</nobr> |
| 642 | xlsx: RIGHTB | <nobr>完了</nobr> |
| 643 | xlsx: ROMAN | <nobr>完了</nobr> |
| 644 | xlsx: ROUND | <nobr>完了</nobr> |
| 645 | xlsx: ROUNDDOWN | <nobr>完了</nobr> |
| 646 | xlsx: ROUNDUP | <nobr>完了</nobr> |
| 647 | xlsx: ROW | <nobr>完了</nobr> |
| 648 | xlsx: ROWS | <nobr>完了</nobr> |
| 649 | xlsx: RRI | <nobr>完了</nobr> |
| 650 | xlsx: RSQ | <nobr>完了</nobr> |
| 651 | xlsx: SCAN | <nobr>完了</nobr> |
| 652 | xlsx: SEARCH | <nobr>完了</nobr> |
| 653 | xlsx: SEARCHB | <nobr>完了</nobr> |
| 654 | xlsx: SEC | <nobr>完了</nobr> |
| 655 | xlsx: SECH | <nobr>完了</nobr> |
| 656 | xlsx: SECOND | <nobr>完了</nobr> |
| 657 | xlsx: SEQUENCE | <nobr>完了</nobr> |
| 658 | xlsx: SERIESSUM | <nobr>一部</nobr> |
| 659 | xlsx: SHEET | <nobr>完了</nobr> |
| 660 | xlsx: SHEETS | <nobr>完了</nobr> |
| 661 | xlsx: SIGN | <nobr>完了</nobr> |
| 662 | xlsx: SIN | <nobr>完了</nobr> |
| 663 | xlsx: SINH | <nobr>完了</nobr> |
| 664 | xlsx: SKEW | <nobr>完了</nobr> |
| 665 | xlsx: SKEW.P | <nobr>完了</nobr> |
| 666 | xlsx: SLN | <nobr>完了</nobr> |
| 667 | xlsx: SLOPE | <nobr>完了</nobr> |
| 668 | xlsx: SMALL | <nobr>完了</nobr> |
| 669 | xlsx: SORT | <nobr>完了</nobr> |
| 670 | xlsx: SORTBY | <nobr>完了</nobr> |
| 671 | xlsx: SQRT | <nobr>完了</nobr> |
| 672 | xlsx: SQRTPI | <nobr>完了</nobr> |
| 673 | xlsx: STANDARDIZE | <nobr>完了</nobr> |
| 674 | xlsx: STDEV | <nobr>完了</nobr> |
| 675 | xlsx: STDEV.P | <nobr>完了</nobr> |
| 676 | xlsx: STDEV.S | <nobr>完了</nobr> |
| 677 | xlsx: STDEVA | <nobr>完了</nobr> |
| 678 | xlsx: STDEVP | <nobr>完了</nobr> |
| 679 | xlsx: STDEVPA | <nobr>完了</nobr> |
| 680 | xlsx: STEYX | <nobr>完了</nobr> |
| 681 | xlsx: SUBSTITUTE | <nobr>完了</nobr> |
| 682 | xlsx: SUBTOTAL | <nobr>完了</nobr> |
| 683 | xlsx: SUM | <nobr>完了</nobr> |
| 684 | xlsx: SUMIF | <nobr>完了</nobr> |
| 685 | xlsx: SUMIFS | <nobr>完了</nobr> |
| 686 | xlsx: SUMPRODUCT | <nobr>完了</nobr> |
| 687 | xlsx: SUMSQ | <nobr>完了</nobr> |
| 688 | xlsx: SUMX2MY2 | <nobr>完了</nobr> |
| 689 | xlsx: SUMX2PY2 | <nobr>完了</nobr> |
| 690 | xlsx: SUMXMY2 | <nobr>完了</nobr> |
| 691 | xlsx: SWITCH | <nobr>完了</nobr> |
| 692 | xlsx: SYD | <nobr>完了</nobr> |
| 693 | xlsx: T | <nobr>完了</nobr> |
| 694 | xlsx: T.DIST | <nobr>一部</nobr> |
| 695 | xlsx: T.DIST.2T | <nobr>一部</nobr> |
| 696 | xlsx: T.DIST.RT | <nobr>一部</nobr> |
| 697 | xlsx: T.INV | <nobr>一部</nobr> |
| 698 | xlsx: T.INV.2T | <nobr>一部</nobr> |
| 699 | xlsx: T.TEST | <nobr>一部</nobr> |
| 700 | xlsx: TAKE | <nobr>完了</nobr> |
| 701 | xlsx: TAN | <nobr>完了</nobr> |
| 702 | xlsx: TANH | <nobr>完了</nobr> |
| 703 | xlsx: TBILLEQ | <nobr>完了</nobr> |
| 704 | xlsx: TBILLPRICE | <nobr>完了</nobr> |
| 705 | xlsx: TBILLYIELD | <nobr>完了</nobr> |
| 706 | xlsx: TDIST | <nobr>一部</nobr> |
| 707 | xlsx: TEXTAFTER | <nobr>完了</nobr> |
| 708 | xlsx: TEXTBEFORE | <nobr>完了</nobr> |
| 709 | xlsx: TEXTJOIN | <nobr>完了</nobr> |
| 710 | xlsx: TEXTSPLIT | <nobr>完了</nobr> |
| 711 | xlsx: TIME | <nobr>完了</nobr> |
| 712 | xlsx: TIMEVALUE | <nobr>完了</nobr> |
| 713 | xlsx: TINV | <nobr>一部</nobr> |
| 714 | xlsx: TOCOL | <nobr>一部</nobr> |
| 715 | xlsx: TODAY | <nobr>完了</nobr> |
| 716 | xlsx: TOROW | <nobr>一部</nobr> |
| 717 | xlsx: TRANSPOSE | <nobr>一部</nobr> |
| 718 | xlsx: TREND | <nobr>一部</nobr> |
| 719 | xlsx: TRIM | <nobr>完了</nobr> |
| 720 | xlsx: TRIMMEAN | <nobr>完了</nobr> |
| 721 | xlsx: TRIMRANGE | <nobr>完了</nobr> |
| 722 | xlsx: TRUE | <nobr>完了</nobr> |
| 723 | xlsx: TRUNC | <nobr>完了</nobr> |
| 724 | xlsx: TTEST | <nobr>一部</nobr> |
| 725 | xlsx: TYPE | <nobr>完了</nobr> |
| 726 | xlsx: UNICHAR | <nobr>完了</nobr> |
| 727 | xlsx: UNICODE | <nobr>完了</nobr> |
| 728 | xlsx: UNIQUE | <nobr>完了</nobr> |
| 729 | xlsx: UPPER | <nobr>完了</nobr> |
| 730 | xlsx: VALUE | <nobr>完了</nobr> |
| 731 | xlsx: VALUETOTEXT | <nobr>完了</nobr> |
| 732 | xlsx: VAR | <nobr>完了</nobr> |
| 733 | xlsx: VAR.P | <nobr>完了</nobr> |
| 734 | xlsx: VAR.S | <nobr>完了</nobr> |
| 735 | xlsx: VARA | <nobr>完了</nobr> |
| 736 | xlsx: VARP | <nobr>完了</nobr> |
| 737 | xlsx: VARPA | <nobr>完了</nobr> |
| 738 | xlsx: VDB | <nobr>一部</nobr> |
| 739 | xlsx: VLOOKUP | <nobr>完了</nobr> |
| 740 | xlsx: VSTACK | <nobr>完了</nobr> |
| 741 | xlsx: WEBSERVICE | <nobr>完了</nobr> |
| 742 | xlsx: WEEKDAY | <nobr>完了</nobr> |
| 743 | xlsx: WEEKNUM | <nobr>完了</nobr> |
| 744 | xlsx: WEIBULL | <nobr>一部</nobr> |
| 745 | xlsx: WEIBULL.DIST | <nobr>一部</nobr> |
| 746 | xlsx: WORKDAY | <nobr>完了</nobr> |
| 747 | xlsx: WORKDAY.INTL | <nobr>完了</nobr> |
| 748 | xlsx: WRAPCOLS | <nobr>完了</nobr> |
| 749 | xlsx: WRAPROWS | <nobr>完了</nobr> |
| 750 | xlsx: XIRR | <nobr>一部</nobr> |
| 751 | xlsx: XLOOKUP | <nobr>完了</nobr> |
| 752 | xlsx: XMATCH | <nobr>完了</nobr> |
| 753 | xlsx: XNPV | <nobr>完了</nobr> |
| 754 | xlsx: XOR | <nobr>完了</nobr> |
| 755 | xlsx: YEAR | <nobr>完了</nobr> |
| 756 | xlsx: YEARFRAC | <nobr>完了</nobr> |
| 757 | xlsx: YIELD | <nobr>一部</nobr> |
| 758 | xlsx: YIELDDISC | <nobr>完了</nobr> |
| 759 | xlsx: YIELDMAT | <nobr>一部</nobr> |
| 760 | xlsx: Z.TEST | <nobr>一部</nobr> |
| 761 | xlsx: ZTEST | <nobr>一部</nobr> |
| 762 | xlsx: 基本図形・線・コネクタの挿入 | <nobr>完了</nobr> |
| 763 | xlsx: 図形の変更と頂点編集 | <nobr>完了</nobr> |
| 764 | xlsx: 図形の塗り・線・影・3D効果 | <nobr>完了</nobr> |
| 765 | xlsx: 図形の位置・サイズ・回転 | <nobr>完了</nobr> |
| 766 | xlsx: オブジェクトの前後順 | <nobr>完了</nobr> |
| 767 | xlsx: 整列・均等配置 | <nobr>完了</nobr> |
| 768 | xlsx: グループ化と解除 | <nobr>完了</nobr> |
| 769 | xlsx: セルと共に移動・サイズ変更の配置 | <nobr>完了</nobr> |
| 770 | xlsx: オブジェクトの印刷指定 | <nobr>完了</nobr> |
| 771 | xlsx: テキストボックス | <nobr>完了</nobr> |
| 772 | xlsx: テキストボックス内の文字・段落書式 | <nobr>完了</nobr> |
| 773 | xlsx: 文字方向・上下配置・内余白 | <nobr>完了</nobr> |
| 774 | xlsx: テキストボックスとセルの連動 | <nobr>完了</nobr> |
| 775 | xlsx: セル上の浮動画像を挿入 | <nobr>完了</nobr> |
| 776 | xlsx: 画像のサイズ・縦横比・回転 | <nobr>完了</nobr> |
| 777 | xlsx: 画像の切抜き | <nobr>完了</nobr> |
| 778 | xlsx: 画像補正・色・透明度 | <nobr>完了</nobr> |
| 779 | xlsx: 画像効果・枠線 | <nobr>完了</nobr> |
| 780 | xlsx: SVG挿入と色変更 | <nobr>完了</nobr> |
| 781 | xlsx: 画像の代替テキスト | <nobr>完了</nobr> |
| 782 | xlsx: セル・グラフ・オブジェクトを画像としてコピー | <nobr>完了</nobr> |
| 783 | xlsx: 標準表示 | <nobr>完了</nobr> |
| 784 | xlsx: 改ページプレビュー | <nobr>完了</nobr> |
| 785 | xlsx: ズーム倍率 | <nobr>完了</nobr> |
| 786 | xlsx: 選択範囲に合わせたズーム | <nobr>完了</nobr> |
| 787 | xlsx: 数式バーとステータスバー | <nobr>完了</nobr> |
| 788 | xlsx: グリッド線・見出し | <nobr>完了</nobr> |
| 789 | xlsx: 先頭行・列・任意位置で枠固定 | <nobr>完了</nobr> |
| 790 | xlsx: ウィンドウ分割 | <nobr>完了</nobr> |
| 791 | xlsx: ウィンドウの非表示・再表示 | <nobr>完了</nobr> |
| 792 | xlsx: カスタムビューの登録 | <nobr>完了</nobr> |
| 793 | xlsx: カスタムビューの表示・削除 | <nobr>完了</nobr> |
| 794 | xlsx: カスタムビューとテーブルの排他条件 | <nobr>完了</nobr> |
| 795 | xlsx: 空白ブックの新規作成 | <nobr>完了</nobr> |
| 796 | xlsx: 既存ブックを開く | <nobr>完了</nobr> |
| 797 | xlsx: 最近使ったブック | <nobr>完了</nobr> |
| 798 | xlsx: テンプレートから新規作成 | <nobr>完了</nobr> |
| 799 | xlsx: 保存・名前を付けて保存 | <nobr>完了</nobr> |
| 800 | xlsx: コピー保存 | <nobr>完了</nobr> |
| 801 | xlsx: xltxテンプレート保存 | <nobr>完了</nobr> |
| 802 | xlsx: CSVの文字コードと区切り | <nobr>完了</nobr> |
| 803 | xlsx: タイトル・著者等のプロパティ | <nobr>完了</nobr> |
| 804 | xlsx: カスタムプロパティ | <nobr>完了</nobr> |
| 805 | xlsx: 折れ線型 | <nobr>完了</nobr> |
| 806 | xlsx: 縦棒型 | <nobr>完了</nobr> |
| 807 | xlsx: 勝敗型 | <nobr>完了</nobr> |
| 808 | xlsx: データ範囲と表示セル | <nobr>完了</nobr> |
| 809 | xlsx: 単一スパークラインの元範囲変更 | <nobr>完了</nobr> |
| 810 | xlsx: グループ作成と解除 | <nobr>完了</nobr> |
| 811 | xlsx: グループの共通書式 | <nobr>完了</nobr> |
| 812 | xlsx: スタイルと線色 | <nobr>完了</nobr> |
| 813 | xlsx: 線の太さ | <nobr>完了</nobr> |
| 814 | xlsx: 通常のマーカー | <nobr>完了</nobr> |
| 815 | xlsx: 高値・安値マーカー | <nobr>完了</nobr> |
| 816 | xlsx: 先頭・最終マーカー | <nobr>完了</nobr> |
| 817 | xlsx: 負の値の強調色 | <nobr>完了</nobr> |
| 818 | xlsx: マーカー色 | <nobr>完了</nobr> |
| 819 | xlsx: 一般軸と日付軸 | <nobr>完了</nobr> |
| 820 | xlsx: 横軸表示 | <nobr>完了</nobr> |
| 821 | xlsx: 右から左の描画 | <nobr>完了</nobr> |
| 822 | xlsx: 縦軸の個別自動範囲 | <nobr>完了</nobr> |
| 823 | xlsx: 縦軸のグループ共通範囲 | <nobr>完了</nobr> |
| 824 | xlsx: 縦軸の最小最大固定値 | <nobr>完了</nobr> |
| 825 | xlsx: 空白セルの間隔・ゼロ・線で接続 | <nobr>完了</nobr> |
| 826 | xlsx: 非表示行列のデータ表示 | <nobr>完了</nobr> |
| 827 | xlsx: スパークラインの消去 | <nobr>完了</nobr> |
| 828 | xlsx: ホーム／挿入／描画／ページレイアウト／数式／データ／校閲／表示／Developerを照合 | <nobr>一部</nobr> |
| 829 | xlsx: セル・行列・シート・オブジェクトの右クリックを照合 | <nobr>一部</nobr> |
| 830 | xlsx: 数式計算・エラーチェック設定を詳細照合 | <nobr>一部</nobr> |

### pptx

| # | 内容 | <nobr>状態</nobr> |
|---|---|---|
| 1 | pptx: 新規スライドとレイアウト指定 | <nobr>完了</nobr> |
| 2 | pptx: スライド複製 | <nobr>完了</nobr> |
| 3 | pptx: スライド削除 | <nobr>完了</nobr> |
| 4 | pptx: スライドの並べ替え | <nobr>完了</nobr> |
| 5 | pptx: スライドのコピーと貼付 | <nobr>完了</nobr> |
| 6 | pptx: 元の書式を保持して再利用 | <nobr>完了</nobr> |
| 7 | pptx: 貼付先テーマで再利用 | <nobr>完了</nobr> |
| 8 | pptx: スライドの非表示 | <nobr>完了</nobr> |
| 9 | pptx: スライドサイズと縦横方向 | <nobr>完了</nobr> |
| 10 | pptx: サイズ変更時の最大化・サイズ調整 | <nobr>完了</nobr> |
| 11 | pptx: セクション作成と名前変更 | <nobr>完了</nobr> |
| 12 | pptx: セクション移動と削除 | <nobr>完了</nobr> |
| 13 | pptx: セクションの折畳み | <nobr>完了</nobr> |
| 14 | pptx: スライド番号の開始値 | <nobr>完了</nobr> |
| 15 | pptx: 複数マスター | <nobr>一部</nobr> |
| 16 | pptx: レイアウトの適用と再適用 | <nobr>一部</nobr> |
| 17 | pptx: スライドのレイアウト初期化 | <nobr>完了</nobr> |
| 18 | pptx: タイトルプレースホルダー | <nobr>一部</nobr> |
| 19 | pptx: 本文・コンテンツプレースホルダー | <nobr>完了</nobr> |
| 20 | pptx: 画像プレースホルダー | <nobr>完了</nobr> |
| 21 | pptx: 表・グラフ・SmartArt用プレースホルダー | <nobr>完了</nobr> |
| 22 | pptx: 動画用プレースホルダー | <nobr>完了</nobr> |
| 23 | pptx: プレースホルダーの位置とサイズ | <nobr>一部</nobr> |
| 24 | pptx: プレースホルダーの案内文字 | <nobr>完了</nobr> |
| 25 | pptx: 階層別の本文スタイル | <nobr>一部</nobr> |
| 26 | pptx: マスターからの書式継承 | <nobr>一部</nobr> |
| 27 | pptx: 背景グラフィックを非表示 | <nobr>完了</nobr> |
| 28 | pptx: 日付・フッター・番号の表示 | <nobr>一部</nobr> |
| 29 | pptx: タイトルスライドのフッター除外 | <nobr>完了</nobr> |
| 30 | pptx: テーマの適用 | <nobr>完了</nobr> |
| 31 | pptx: テーマのバリエーション | <nobr>完了</nobr> |
| 32 | pptx: テーマ色の編集 | <nobr>完了</nobr> |
| 33 | pptx: テーマフォントの編集 | <nobr>完了</nobr> |
| 34 | pptx: 背景の単色塗り | <nobr>完了</nobr> |
| 35 | pptx: 背景のグラデーション | <nobr>完了</nobr> |
| 36 | pptx: 背景の画像・テクスチャ | <nobr>完了</nobr> |
| 37 | pptx: 背景のパターン | <nobr>完了</nobr> |
| 38 | pptx: 背景を全スライドへ適用 | <nobr>完了</nobr> |
| 39 | pptx: テンプレートの保存と利用 | <nobr>完了</nobr> |
| 40 | pptx: 基本図形の挿入 | <nobr>完了</nobr> |
| 41 | pptx: 線とコネクタ | <nobr>完了</nobr> |
| 42 | pptx: 図形の変更 | <nobr>完了</nobr> |
| 43 | pptx: 自由曲線・フリーフォーム | <nobr>一部</nobr> |
| 44 | pptx: 図形の塗り | <nobr>完了</nobr> |
| 45 | pptx: 線の色・幅・種類 | <nobr>完了</nobr> |
| 46 | pptx: 影・反射・光彩 | <nobr>完了</nobr> |
| 47 | pptx: ぼかし・面取り | <nobr>完了</nobr> |
| 48 | pptx: 3D回転と奥行き | <nobr>一部</nobr> |
| 49 | pptx: 位置・サイズの数値指定 | <nobr>完了</nobr> |
| 50 | pptx: 縦横比固定 | <nobr>完了</nobr> |
| 51 | pptx: 回転と反転 | <nobr>完了</nobr> |
| 52 | pptx: 前面・背面の重なり順 | <nobr>完了</nobr> |
| 53 | pptx: 整列と等間隔配置 | <nobr>完了</nobr> |
| 54 | pptx: グループ化と解除 | <nobr>完了</nobr> |
| 55 | pptx: オブジェクト名 | <nobr>完了</nobr> |
| 56 | pptx: 選択ウィンドウと表示切替 | <nobr>完了</nobr> |
| 57 | pptx: 図形の既定書式 | <nobr>完了</nobr> |
| 58 | pptx: ハイパーリンク | <nobr>完了</nobr> |
| 59 | pptx: アクションボタンとクリック動作 | <nobr>完了</nobr> |
| 60 | pptx: テキストボックス | <nobr>完了</nobr> |
| 61 | pptx: WordArt | <nobr>一部</nobr> |
| 62 | pptx: フォントとサイズ | <nobr>完了</nobr> |
| 63 | pptx: 太字・斜体・下線 | <nobr>完了</nobr> |
| 64 | pptx: 文字色と蛍光ペン | <nobr>完了</nobr> |
| 65 | pptx: 文字の輪郭・効果 | <nobr>完了</nobr> |
| 66 | pptx: 取り消し線 | <nobr>完了</nobr> |
| 67 | pptx: 上付き・下付き | <nobr>完了</nobr> |
| 68 | pptx: 大文字小文字変換 | <nobr>完了</nobr> |
| 69 | pptx: 段落の左・中央・右・両端配置 | <nobr>完了</nobr> |
| 70 | pptx: 均等割付 | <nobr>完了</nobr> |
| 71 | pptx: 箇条書き記号と画像 | <nobr>完了</nobr> |
| 72 | pptx: 番号付きリスト | <nobr>完了</nobr> |
| 73 | pptx: リスト階層 | <nobr>完了</nobr> |
| 74 | pptx: 段落インデントとタブ | <nobr>完了</nobr> |
| 75 | pptx: 行間と段落前後の間隔 | <nobr>完了</nobr> |
| 76 | pptx: テキストの段組 | <nobr>完了</nobr> |
| 77 | pptx: 縦書きと回転 | <nobr>完了</nobr> |
| 78 | pptx: 文字の上下配置 | <nobr>完了</nobr> |
| 79 | pptx: テキストボックス内余白 | <nobr>完了</nobr> |
| 80 | pptx: 自動縮小と図形サイズ調整 | <nobr>完了</nobr> |
| 81 | pptx: 図形内の折返し | <nobr>完了</nobr> |
| 82 | pptx: 記号の挿入 | <nobr>完了</nobr> |
| 83 | pptx: 書式コピーとクリア | <nobr>完了</nobr> |
| 84 | pptx: 検索・置換・フォント置換 | <nobr>完了</nobr> |
| 85 | pptx: 行列数を指定して表作成 | <nobr>完了</nobr> |
| 86 | pptx: 行列の挿入・削除 | <nobr>完了</nobr> |
| 87 | pptx: セル結合と分割 | <nobr>完了</nobr> |
| 88 | pptx: 表全体の移動とサイズ | <nobr>完了</nobr> |
| 89 | pptx: 行高・列幅 | <nobr>完了</nobr> |
| 90 | pptx: 行列の均等配置 | <nobr>完了</nobr> |
| 91 | pptx: セル内上下配置 | <nobr>完了</nobr> |
| 92 | pptx: セル余白 | <nobr>完了</nobr> |
| 93 | pptx: セルの文字方向 | <nobr>完了</nobr> |
| 94 | pptx: 罫線の線種・色・幅 | <nobr>完了</nobr> |
| 95 | pptx: セルの塗りと効果 | <nobr>完了</nobr> |
| 96 | pptx: 表スタイル | <nobr>完了</nobr> |
| 97 | pptx: 見出し・集計行と縞模様 | <nobr>完了</nobr> |
| 98 | pptx: 表内文字の書式 | <nobr>完了</nobr> |
| 99 | pptx: 縦棒・横棒 | <nobr>完了</nobr> |
| 100 | pptx: 折れ線・面 | <nobr>完了</nobr> |
| 101 | pptx: 円・ドーナツ | <nobr>完了</nobr> |
| 102 | pptx: 散布図・バブル | <nobr>完了</nobr> |
| 103 | pptx: 株価・レーダー・表面 | <nobr>一部</nobr> |
| 104 | pptx: 組合せと第2軸 | <nobr>一部</nobr> |
| 105 | pptx: 埋込ワークシートの編集 | <nobr>一部</nobr> |
| 106 | pptx: 系列とデータ範囲 | <nobr>一部</nobr> |
| 107 | pptx: グラフタイトルと凡例 | <nobr>完了</nobr> |
| 108 | pptx: 軸の範囲と目盛 | <nobr>完了</nobr> |
| 109 | pptx: データラベル | <nobr>完了</nobr> |
| 110 | pptx: 系列とプロット領域の書式 | <nobr>完了</nobr> |
| 111 | pptx: 誤差範囲と近似曲線 | <nobr>完了</nobr> |
| 112 | pptx: グラフスタイル・配色 | <nobr>完了</nobr> |
| 113 | pptx: リスト型 | <nobr>一部</nobr> |
| 114 | pptx: ローカル画像の挿入 | <nobr>完了</nobr> |
| 115 | pptx: 画像の置換 | <nobr>完了</nobr> |
| 116 | pptx: サイズと縦横比固定 | <nobr>完了</nobr> |
| 117 | pptx: 回転・反転 | <nobr>完了</nobr> |
| 118 | pptx: 切り抜きと縦横比 | <nobr>完了</nobr> |
| 119 | pptx: 図形に合わせた切り抜き | <nobr>完了</nobr> |
| 120 | pptx: 明るさ・コントラスト補正 | <nobr>完了</nobr> |
| 121 | pptx: 色と透明度 | <nobr>完了</nobr> |
| 122 | pptx: 画像の枠線と効果 | <nobr>完了</nobr> |
| 123 | pptx: 画像の初期化 | <nobr>完了</nobr> |
| 124 | pptx: SVGの挿入と色変更 | <nobr>完了</nobr> |
| 125 | pptx: GIFの表示と再生 | <nobr>一部</nobr> |
| 126 | pptx: 端末上の動画を埋込 | <nobr>完了</nobr> |
| 127 | pptx: 動画の再生開始条件 | <nobr>完了</nobr> |
| 128 | pptx: 動画の開始・終了トリミング | <nobr>完了</nobr> |
| 129 | pptx: フェードイン・フェードアウト | <nobr>完了</nobr> |
| 130 | pptx: 音量 | <nobr>完了</nobr> |
| 131 | pptx: 全画面再生 | <nobr>完了</nobr> |
| 132 | pptx: 再生後に巻戻す | <nobr>完了</nobr> |
| 133 | pptx: 連続再生 | <nobr>完了</nobr> |
| 134 | pptx: 再生していない時に非表示 | <nobr>完了</nobr> |
| 135 | pptx: ポスターフレーム | <nobr>完了</nobr> |
| 136 | pptx: 再生ブックマーク | <nobr>完了</nobr> |
| 137 | pptx: 端末上の音声挿入 | <nobr>完了</nobr> |
| 138 | pptx: 自動・クリックでの再生 | <nobr>完了</nobr> |
| 139 | pptx: バックグラウンド再生 | <nobr>完了</nobr> |
| 140 | pptx: スライドをまたいで再生 | <nobr>完了</nobr> |
| 141 | pptx: 繰返し再生 | <nobr>完了</nobr> |
| 142 | pptx: 開始・終了トリミング | <nobr>完了</nobr> |
| 143 | pptx: フェードイン・フェードアウト | <nobr>完了</nobr> |
| 144 | pptx: 音量とミュート | <nobr>完了</nobr> |
| 145 | pptx: 音声アイコンの表示切替 | <nobr>完了</nobr> |
| 146 | pptx: 音声のブックマーク | <nobr>完了</nobr> |
| 147 | pptx: 終了後の巻戻し | <nobr>完了</nobr> |
| 148 | pptx: 開始効果 | <nobr>一部</nobr> |
| 149 | pptx: 強調効果 | <nobr>一部</nobr> |
| 150 | pptx: 終了効果 | <nobr>一部</nobr> |
| 151 | pptx: 軌跡効果 | <nobr>完了</nobr> |
| 152 | pptx: 効果方向とオプション | <nobr>完了</nobr> |
| 153 | pptx: 複数効果の追加 | <nobr>完了</nobr> |
| 154 | pptx: クリック時に開始 | <nobr>完了</nobr> |
| 155 | pptx: 直前と同時に開始 | <nobr>完了</nobr> |
| 156 | pptx: 直前の後に開始 | <nobr>完了</nobr> |
| 157 | pptx: 開始遅延 | <nobr>完了</nobr> |
| 158 | pptx: 開始・終了の滑らかさ | <nobr>完了</nobr> |
| 159 | pptx: 効果の順序変更 | <nobr>完了</nobr> |
| 160 | pptx: アニメーションウィンドウ | <nobr>完了</nobr> |
| 161 | pptx: 詳細タイムライン | <nobr>完了</nobr> |
| 162 | pptx: 効果終了時の色・非表示 | <nobr>完了</nobr> |
| 163 | pptx: アニメーションのコピー | <nobr>完了</nobr> |
| 164 | pptx: 効果のプレビューと削除 | <nobr>完了</nobr> |
| 165 | pptx: 画面切替の設定 | <nobr>完了</nobr> |
| 166 | pptx: 画面切替の効果オプション | <nobr>完了</nobr> |
| 167 | pptx: 切替の継続時間 | <nobr>完了</nobr> |
| 168 | pptx: 切替音 | <nobr>完了</nobr> |
| 169 | pptx: クリックでスライド進行 | <nobr>完了</nobr> |
| 170 | pptx: 時間指定で自動進行 | <nobr>完了</nobr> |
| 171 | pptx: 切替を全スライドへ適用 | <nobr>完了</nobr> |
| 172 | pptx: 先頭から開始 | <nobr>完了</nobr> |
| 173 | pptx: 現在のスライドから開始 | <nobr>完了</nobr> |
| 174 | pptx: 目的別スライドショー | <nobr>完了</nobr> |
| 175 | pptx: 発表者・閲覧者・キオスクの設定 | <nobr>完了</nobr> |
| 176 | pptx: 連続実行 | <nobr>完了</nobr> |
| 177 | pptx: 表示するスライド範囲 | <nobr>完了</nobr> |
| 178 | pptx: ナレーション・アニメーションの抑制 | <nobr>完了</nobr> |
| 179 | pptx: タイミングを使う進行 | <nobr>完了</nobr> |
| 180 | pptx: 発表者ツール | <nobr>完了</nobr> |
| 181 | pptx: 現在・次スライドの表示 | <nobr>完了</nobr> |
| 182 | pptx: 発表者ノート | <nobr>完了</nobr> |
| 183 | pptx: 時間計測と一時停止 | <nobr>完了</nobr> |
| 184 | pptx: 全スライド一覧 | <nobr>完了</nobr> |
| 185 | pptx: スライドの拡大 | <nobr>完了</nobr> |
| 186 | pptx: レーザーポインター | <nobr>完了</nobr> |
| 187 | pptx: ペンと蛍光ペン | <nobr>完了</nobr> |
| 188 | pptx: 画面を黒・白にする | <nobr>完了</nobr> |
| 189 | pptx: モニターの選択と入替 | <nobr>完了</nobr> |
| 190 | pptx: タイミングのリハーサル | <nobr>完了</nobr> |
| 191 | pptx: スピーカーノートの入力 | <nobr>完了</nobr> |
| 192 | pptx: ノートマスター | <nobr>一部</nobr> |
| 193 | pptx: 標準表示 | <nobr>完了</nobr> |
| 194 | pptx: アウトライン表示 | <nobr>完了</nobr> |
| 195 | pptx: スライド一覧 | <nobr>完了</nobr> |
| 196 | pptx: ノート表示 | <nobr>完了</nobr> |
| 197 | pptx: 閲覧表示 | <nobr>完了</nobr> |
| 198 | pptx: ズームとウィンドウ整列 | <nobr>完了</nobr> |
| 199 | pptx: グリッド・ガイド・スナップ | <nobr>完了</nobr> |
| 200 | pptx: テレプロンプターとノート | <nobr>一部</nobr> |
| 201 | pptx: PDFとタグの出力 | <nobr>完了</nobr> |
| 202 | pptx: 画像へのスライド出力 | <nobr>完了</nobr> |
| 203 | pptx: スライドショー形式への保存 | <nobr>完了</nobr> |
| 204 | pptx: 校正言語 | <nobr>完了</nobr> |
| 205 | pptx: コメント追加と削除 | <nobr>完了</nobr> |
| 206 | pptx: コメント返信 | <nobr>完了</nobr> |
| 207 | pptx: コメント解決と再開 | <nobr>完了</nobr> |
| 208 | pptx: 読取り専用と最終版表示 | <nobr>完了</nobr> |
| 209 | pptx: 文書検査と個人情報除去 | <nobr>完了</nobr> |
| 210 | pptx: アクセシビリティ検査 | <nobr>完了</nobr> |
| 211 | pptx: スライドの一意なタイトル | <nobr>完了</nobr> |
| 212 | pptx: 非表示タイトル | <nobr>完了</nobr> |
| 213 | pptx: 読み上げ順序 | <nobr>完了</nobr> |
| 214 | pptx: 画像・図形・グラフの代替テキスト | <nobr>完了</nobr> |
| 215 | pptx: 装飾オブジェクト指定 | <nobr>完了</nobr> |
| 216 | pptx: 表の見出しと読み順 | <nobr>完了</nobr> |
| 217 | pptx: 意味のあるリンク文字 | <nobr>完了</nobr> |
| 218 | pptx: リンクのスクリーンヒント | <nobr>完了</nobr> |
| 219 | pptx: 色コントラスト検査 | <nobr>完了</nobr> |
| 220 | pptx: キーボード操作 | <nobr>一部</nobr> |
| 221 | pptx: 拡大表示と高コントラスト | <nobr>一部</nobr> |
| 222 | pptx: 新規・開く・最近使ったファイル | <nobr>完了</nobr> |
| 223 | pptx: 保存・名前を付けて保存・コピー保存 | <nobr>完了</nobr> |
| 224 | pptx: テンプレートから新規作成 | <nobr>完了</nobr> |
| 225 | pptx: potxテンプレート保存 | <nobr>完了</nobr> |
| 226 | pptx: タイトル・著者等のプロパティ | <nobr>完了</nobr> |
| 227 | pptx: カスタムプロパティ | <nobr>完了</nobr> |
| 228 | pptx: フォントの埋込と代替 | <nobr>一部</nobr> |
| 229 | pptx: 既存features/scenarios/coverageをID対応 | <nobr>一部</nobr> |
| 230 | pptx: 未対応機能の無編集往復保存と代替表示を評価 | <nobr>一部</nobr> |

## ライセンス

ibisOffice は、**AGPL-3.0-only と商用ライセンスのデュアルライセンス**です。

- **AGPL-3.0**：オープンソースとして、自由に使い、改変し、配布できます。改変したものをネットワーク越しにサービスとして提供する場合も、そのソースを AGPL-3.0 で公開する必要があります。本文は [LICENSE](LICENSE) にあります。
- **商用ライセンス**：AGPL-3.0 の条件（ソースの公開）を満たせない製品やサービスに組み込む場合は、商用ライセンスをご利用ください。案内は [COMMERCIAL-LICENSE.md](COMMERCIAL-LICENSE.md) にあります。

ibisOffice の著作権は、すべて株式会社アイビスが持っています。そのため、両方のライセンスで提供できます。

使っている部品と、それぞれのライセンスが求める表示は [www/license.md](www/license.md) にあります。GPUI（Apache-2.0）は、改変したフォークを [gpui-pre](https://github.com/ibisjp/gpui-pre)・[gpui-pre-macos](https://github.com/ibisjp/gpui-pre-macos)・[gpui-pre-web](https://github.com/ibisjp/gpui-pre-web) で公開しています。

## 貢献について

貢献者ライセンス同意書（CLA）の準備ができるまで、外部からの貢献（プル リクエスト）は受け付けていません。不具合の報告は Issues で歓迎します。セキュリティの問題は、Issues に書かず [SECURITY.md](SECURITY.md) の窓口から知らせてください。

## 商標について

Microsoft、Microsoft Office、Microsoft 365、Word、Excel、PowerPoint は、米国 Microsoft Corporation の米国およびその他の国における登録商標または商標です。LibreOffice は The Document Foundation の、ONLYOFFICE は Ascensio System SIA の登録商標または商標です。その他、記載されている会社名・製品名は、各社の登録商標または商標です。ibisOffice は、これらの会社とは関係がなく、承認や後援を受けたものではありません。

「ibisOffice」の名前とロゴは株式会社アイビスの商標です。使い方は [TRADEMARK.md](TRADEMARK.md) をご覧ください。ibisOffice をもとにしたものには「Powered by ibisOffice」の表示が必要です。

---

© 株式会社アイビス（ibis inc.）
