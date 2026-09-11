<div align="center">

[English](README.md) | [한국어](README.ko.md) | [日本語](README.ja.md)

  <img src="public/icon.png" alt="KeyGlow アイコン" width="112" />

# KeyGlow

### キーボードを見て、すべてのキーをコントロール。

Windows 向けの軽量・ローカルファーストなビジュアルキーボードコントローラーです。

画面上のキーボードで任意のキーをクリックすると無効化でき、もう一度クリックすればすぐに有効化できます。KeyGlow は UI 上の表示だけでなく、Windows の入力レイヤーで実際のキー入力を制御します。

[![Windows 10/11](https://img.shields.io/badge/Windows-10%20%7C%2011-0078D4?logo=windows11&logoColor=white)](#対応プラットフォーム)
[![Tauri 2](https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white)](https://tauri.app/)
[![Rust](https://img.shields.io/badge/Rust-native%20input-000000?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![React](https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=111)](https://react.dev/)
[![Local First](https://img.shields.io/badge/privacy-local--first-B6F25C)](#プライバシー)

</div>

---

## なぜ KeyGlow？

多くのキーボード設定ソフトは特定メーカーのハードウェアに依存しています。KeyGlow は Logitech、Razer、Keychron、Leopold、Akko など特定ブランドに縛られない汎用ビジュアルコントローラーを目指しています。

使用中の実キーボードに最も近いキーボードタイプを選び、画面上で各キーを直接コントロールできます。

- キーをクリック → 無効化
- もう一度クリック → 有効化
- 実際のキー入力 → UI 上でリアルタイム点灯
- 再起動なしでキーボードタイプを切り替え
- 作業用 / ゲーム用 / カスタムプロファイルを保存
- アカウント、テレメトリ、クラウド不要でローカル動作

> KeyGlow はキーボードの見た目だけを再現するモックではありません。無効化されたキーは Windows の低レベルキーボードフックで実際にブロックされます。

## インターフェース

KeyGlow の状態表現はシンプルです。

| 状態 | 意味 |
| --- | --- |
| 🟢 発光 | 有効なキー |
| ✨ 強く発光 / 押下 | 現在実際に押されているキー |
| ⚫ 暗い | 無効なキー |

### スクリーンショット

リリースビルドから実際の Windows アプリ画面を撮影し、ここに掲載する予定です。

推奨キャプチャ:

- 約 `1440 × 900` のウィンドウ
- `デスクトップキーボード（テンキーあり）` を選択
- 2〜3 個のキーを無効化
- 可能なら 1 キーが押下中で発光している状態
- Windows スケーリング 100% または 125%
- `docs/assets/keyglow-main.png` として保存

後で以下に置き換えます。

```html
<p align="center">
  <img src="docs/assets/keyglow-main.png" alt="KeyGlow メイン画面" width="100%" />
</p>
```

## 主な機能

### キーごとのビジュアル制御

画面上のキーボードで対応キーをクリックすると、その入力を Windows に渡すかどうかを即座に切り替えられます。

### リアルタイムキーグロー

実キーボードの key-down / key-up イベントを UI に反映し、仮想キーボードをリアルタイムのコントロールサーフェスとして表示します。

### 複数のキーボードタイプ

KeyGlow は物理キーボードのタイプと動作プロファイルを別々に管理します。

| キーボードタイプ | 説明 |
| --- | --- |
| デスクトップキーボード | テンキー付きの一般的な PC キーボード |
| スリムデスクトップ | テンキーのない一般的なデスクトップキーボード |
| コンパクトキーボード | 矢印キーとファンクションキー列を備えた小型配列 |
| ミニキーボード | 矢印キーあり、独立したファンクションキー列なし |
| ノートPC風キーボード | 独立した矢印キーとファンクションキー列を省いた最小配列 |

ユーザー向け UI では TKL、75%、65%、60% といった愛好家向けの比率表記よりも、形を直感的に理解しやすいキーボードタイプ名を使用します。

内部では `fullsize-ansi`、`tkl-ansi`、`75-ansi`、`65-ansi`、`60-ansi` といったデータ駆動のレイアウト定義を維持しているため、レンダラーを作り直さずに新しいキーボードタイプを追加できます。

### プロファイル

用途ごとに異なるキー状態を保存できます。

- Default
- Gaming
- Coding
- カスタムプロファイル

プロファイルの作成、複製、名称変更、リセット、削除、再起動後の復元に対応します。

### 🐈 Cat Lock

猫、子ども、清掃用クロスなどがキーボードに乗ったとき、すべてのキーを即座にブロックできる機能です。

Cat ボタン、または次の緊急ショートカットで解除できます。

```text
Ctrl + Shift + F12
```

### 緊急解除

`Ctrl + Shift + F12` は通常のプロファイルでは無効化できません。実行するとすべてのキーを即座に有効化し、Default プロファイルに戻ります。

### システムトレイ

メインウィンドウを閉じると KeyGlow はトレイに隠れます。トレイメニューから完全終了するとキーボードフックが解除され、通常の入力状態に戻ります。

### 多言語 UI

現在の対応言語:

- English
- 한국어
- 日本語

## 仕組み

```text
物理キーボード
     ↓
WH_KEYBOARD_LL  (SetWindowsHookExW)
     ↓
KeyGlow FilterEngine
     ↓
 無効化されている？
    /       \
   はい      いいえ
   ↓          ↓
イベント遮断   CallNextHookEx
               ↓
          Windows アプリ
```

キーボードフックのコールバックは意図的に最小限に保っています。

- ディスク I/O なし
- ネットワーク要求なし
- 入力キーの永続ログなし
- フック内部で React 処理なし
- 低コストなインメモリ状態確認とイベント転送のみ

UI 更新は非同期で配信し、キーボードフィルタリングの遅延を抑えます。

## 対応プラットフォーム

| プラットフォーム | 状態 |
| --- | --- |
| Windows 11 | ✅ 対応 |
| Windows 10 | ✅ 対応 |
| macOS | ⏳ 未対応 |
| Linux | ⏳ 未対応 |

Windows 固有の入力コードはネイティブのプラットフォーム層に分離されており、将来の他 OS 対応を拡張しやすい構成です。

## クイックスタート

### 必要環境

- Node.js 20+
- Rust stable 1.77+
- WebView2
- MSVC C++ toolchain を含む Visual Studio Build Tools

### 開発実行

```bash
git clone https://github.com/sapgun/KeyGlow.git
cd KeyGlow
npm install
npm run tauri dev
```

レイアウト生成スクリプト変更後の JSON 再生成:

```bash
npm run gen:layouts
```

全テスト実行:

```bash
npm run test:all
```

## Windows ビルド

```bash
npm run tauri build
```

想定出力:

```text
src-tauri/target/release/bundle/nsis/KeyGlow_0.1.0_x64-setup.exe
release/KeyGlow_0.1.0_x64-setup.exe

src-tauri/target/release/keyglow.exe
release/KeyGlow.exe
```

現在のインストーラーはユーザー単位インストールを使用し、通常利用では管理者権限を必要としません。

## プライバシー

KeyGlow はローカルファーストで設計されています。

- テレメトリなし
- アナリティクスなし
- クラウドアカウントなし
- ネットワーク接続不要
- 入力したキー履歴をディスクに保存しない
- レジストリによる永続的なキー無効化なし

リアルタイム key-down/up イベントは、ローカル UI でキーキャップを光らせる目的にのみ使用されます。

設定は通常、次の場所に保存されます。

```text
%APPDATA%\com.keyglow.app\settings.json
```

## 安全性と制限事項

KeyGlow は通常のユーザーモード内で動作するよう設計されています。

- 現在のフィルタリングは個々の物理キーボードではなく Windows セッション全体に適用されます。
- `Ctrl + Alt + Delete` と Secure Attention Sequence は Windows によって保護されています。
- 多くのキーボードでは `Fn` はファームウェア側で処理されるため、通常の Windows キーとしてフックできません。
- KeyGlow が終了またはクラッシュした場合、フックも消えるため通常のキーボード入力に戻ります。

詳細: [docs/LIMITATIONS.md](docs/LIMITATIONS.md)

## プロジェクトドキュメント

- [Architecture](docs/ARCHITECTURE.md)
- [Known limitations](docs/LIMITATIONS.md)
- [Manual test checklist](docs/TEST-CHECKLIST.md)

## ロードマップ

Windows v0.1 の基盤が安定した後に検討する項目:

- アプリごとのプロファイル
- Raw Input による物理キーボード識別
- ISO / JIS レイアウト
- HHKB / Alice / Split 配列
- カスタムレイアウトのインポート
- キーリマップ
- マクロとレイヤー
- オプションの QMK / VIA 連携

## コントリビューション

Issue、バグ報告、キーボードタイプ/レイアウトの追加、UX フィードバック、Pull Request を歓迎します。

キーボードフック関連のバグを報告する際は、次の情報を含めてください。

- Windows バージョン
- キーボードタイプ / プロファイル
- 問題のあるキー
- 緊急解除後も再現するか

## ❤️ KeyGlow をサポート

KeyGlow が役に立った場合は、Star、再現可能なバグ報告、キーボードレイアウトの貢献、プロジェクト共有、または寄付で開発を支援できます。

[![Ko-fi](https://img.shields.io/badge/Ko--fi-Support%20KeyGlow-FF5E5B?logo=kofi&logoColor=white)](https://ko-fi.com/sapgun)

### Crypto

| ネットワーク / 方法 | 寄付先 |
| --- | --- |
| Ethereum | `0xDF2930264Cf2285eB76C232b3a1233f0c5D4b471` |
| Solana | `BzsE914REG8op1uonEv7rz2NxiS9k3Jcrivz84NdNd5H` |
| Tether ID | `sapgun98@tether.me` |

> 送金前に宛先とネットワークを必ず再確認してください。暗号資産の送金は取り消せません。Ethereum アドレスには Ethereum ネットワーク、Solana アドレスには Solana ネットワークを使用してください。Tether ID は `tether.me` 識別子を明示的にサポートするサービスからのみ利用してください。

PayPal は公開 PayPal 決済リンクまたは PayPal.Me URL が用意でき次第追加予定です。PayPal アカウントのダッシュボード URL は公開寄付リンクではありません。

---

<div align="center">

Built with Tauri · Rust · React · TypeScript

**KeyGlow — キーボードを見て、すべてのキーをコントロール。**

</div>
