# Kitu MVP Architecture Documentation

## Table of Contents
- [TOC](#toc)
- [次にやるとよさそうな詳細化ステップ（候補）](#次にやるとよさそうな詳細化ステップ候補)
- [Kitu ライブラリ構成まとめ（crate / Unity パッケージ）](#kitu-ライブラリ構成まとめcrate-unity-パッケージ)
- [ユースケース一覧](#ユースケース一覧)
- [詳細フロー](#詳細フロー)

## TOC

- [1. 次にやるとよさそうな詳細化ステップ（候補）](#1-次にやるとよさそうな詳細化ステップ候補)
- [2. Kitu ライブラリ構成まとめ（crate / Unity パッケージ）](#2-kitu-ライブラリ構成まとめcrate--unity-パッケージ)
  - [2.1 Rust Workspace 全体構成](#21-rust-workspace-全体構成)
  - [2.2 各 crate の責務](#22-各-crate-の責務)
  - [2.3 ゲームアプリ側リポジトリ構造](#23-ゲームアプリ側リポジトリ構造)
  - [2.4 各 game-\* crate の責務](#24-各-game--crate-の責務)
- [3. ユースケース一覧](#3-ユースケース一覧)
  - [A. 起動・基本ループ](#a-起動基本ループ)
  - [B. プレイヤー操作・移動](#b-プレイヤー操作移動)
  - [C. バトル・敵・ダメージ](#c-バトル敵ダメージ)
  - [D. ステータス・アイテム・レベル](#d-ステータスアイテムレベル)
  - [E. クエスト・フラグ・シナリオ](#e-クエストフラグシナリオ)
  - [F. 演出（TSQ1）](#f-演出tsq1)
  - [G. UI・メニュー](#g-uiメニュー)
  - [H. データ駆動・ホットリロード](#h-データ駆動ホットリロード)
  - [I. デバッグ・ツール・リプレイ](#i-デバッグツールリプレイ)
  - [J. セーブ・ロード](#j-セーブロード)
- [4. UC-01 / UC-02 詳細フロー（WIP）](#4-uc-01--uc-02-詳細フローワークインプログレス)



## 次にやるとよさそうな詳細化ステップ（候補）

この章では、Kitu アーキテクチャを具体的に仕様化・実装する前に整理しておくべきポイントをまとめます。

1. **通信プロトコルの詳細化（OSC / osc-ir / MessagePack）**
2. **Rust バックエンド（Kitu Runtime）の詳細設計**
3. **Unity クライアント（表現レイヤー）の抽象化**
4. **TSQ1 タイムラインの Kitu 仕様**
5. **データ駆動（TMD + SQLite）の統合モデル**
6. **Rhai スクリプトの API 設計**
7. **Shell / Web Admin / リプレイ統合**


## Kitu ライブラリ構成まとめ（crate / Unity パッケージ）

ここでは、過去の会話で議論した **Kitu フレームワークそのものの分割案**をまとめます。

### Rust Workspace 全体構成（kitu リポジトリ）

```
kitu/
  Cargo.toml              # workspace
  crates/
    kitu-core/
    kitu-ecs/
    kitu-osc-ir/
    kitu-transport/
    kitu-runtime/
    kitu-scripting-rhai/
    kitu-data-tmd/
    kitu-data-sqlite/
    kitu-tsq1/
    kitu-shell/
    kitu-web-admin-backend/
    kitu-unity-ffi/
  tools/
    kitu-cli/
    kitu-replay-runner/
  unity/
    com.kitu.runtime/
    com.kitu.transport/
    com.kitu.editor/
  specs/
    tsq1/
    tmd/
    osc-ir/
```

### 各 crate の責務

- **kitu-core**: ID 型、エラー型、時間管理、共通ユーティリティ
- **kitu-ecs**: ECS 抽象レイヤー（bevy\_ecs 等の薄いラッパ）
- **kitu-osc-ir**: OSC 風イベントの型定義（address + args）
- **kitu-transport**: WebSocket / LocalChannel などの送受信抽象
- **kitu-runtime**: tick ベースのゲームループ、入力/出力イベント管理
- **kitu-scripting-rhai**: Rhai スクリプト統合
- **kitu-data-tmd**: TMD フォーマットのパース、構造体化
- **kitu-data-sqlite**: SQLite 管理、スキーマ、参照
- **kitu-tsq1**: TSQ1 の AST / 再生エンジン
- **kitu-shell**: CLI シェル（/debug 用イベント発火など）
- **kitu-web-admin-backend**: Web Admin のバックエンド（HTTP + WS）
- **kitu-unity-ffi**: Unity 組み込み cdylib 用の C API

### ゲームアプリ側（stella-rpg）リポジトリ構造

```
stella-rpg/
  Cargo.toml
  crates/
    game-core/
    game-ecs-features/
    game-data-schema/
    game-data-build/
    game-logic/
    game-timeline/
    game-scripts/
    game-shell-ext/
    game-webadmin-ext/
  data/
    tmd/
    tsq1/
    scripts/
    localization/
  unity/
    com.stella.game/
    com.stella.game.editor/
```

### 各 game-\* crate の責務

- **game-core**: KituRuntime を組み込んだ StellaGame の入口
- **game-ecs-features**: コンポーネント & システム登録
- **game-data-schema**: ゲーム固有データ型の定義（Unit, Item, Skill…）
- **game-data-build**: TMD/SQLite からデータストア構築
- **game-logic**: 戦闘や移動などのゲームルール
- **game-timeline**: ゲーム固有の TSQ1 ハンドリング
- **game-scripts**: Rhai API の公開・ゲームロジック統合
- **game-shell-ext**: Shell 用のゲーム固有コマンド
- **game-webadmin-ext**: Web Admin のゲーム固有ビュー/API


このドキュメントでは、Kitu フレームワークを利用して実現するアプリケーション（テンプレートプロジェクトおよび Stella RPG）のユースケース一覧と、それぞれのアーキテクチャ上の流れ・関与するライブラリを整理します。

## ユースケース一覧

（※ 本ドキュメントはチャットで議論した内容を随時統合して拡張していきます）

### A. 起動・基本ループ

- UC-01: ゲーム起動 & シーン初期化
- UC-02: メインループ（tick ごとのシミュレーション & 表示更新）

### B. プレイヤー操作・移動

- UC-10: プレイヤー移動
- UC-11: カメラ追従

### C. バトル・敵・ダメージ

- UC-20: 敵スポーン
- UC-21: プレイヤー近接攻撃
- UC-22: 敵AI行動
- UC-23: HP減少 & 死亡処理

### D. ステータス・アイテム・レベル

- UC-30: 経験値・レベルアップ
- UC-31: アイテム取得
- UC-32: アイテム使用

### E. クエスト・フラグ・シナリオ

- UC-40: クエスト進行
- UC-41: シナリオフラグ分岐

### F. 演出（TSQ1）

- UC-51: スキル発動演出（短いTSQ1）

### G. UI・メニュー

- UC-60: HUD更新
- UC-61: ポーズ / メニュー

### H. データ駆動・ホットリロード

- UC-70: TMD変更 → 反映
- UC-72: Rhaiスクリプト変更 → 反映

### I. デバッグ・ツール・リプレイ

- UC-80: Shellからデバッグコマンド実行
- UC-81: Web Adminで状態監視
- UC-82: リプレイ（入力再生）
- UC-83: Web AdminからKitu Shellコマンド実行

### J. セーブ・ロード

- UC-90: セーブデータ読み書き


## 詳細フロー

※ 詳細フロー（UC-01 / UC-02）は `kitu_detailed_flows.md` に移動しました。

ここには今後の各 UC のまとめリンクや要約のみを記述します。

