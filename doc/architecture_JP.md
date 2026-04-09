# Kitu MVP Architecture Documentation

> 同期ポリシー（2026-04-09 更新）:
> - 本ドキュメントの正本は英語版 `doc/architecture.md` です。
> - 英語版の変更に追従する際は、同一PR/コミットで本ファイルも更新してください。
> - 同期を後回しにする場合は `PROJECT_TODO.md` に追跡タスクを追加してください。

## Table of Contents
- [次にやるとよさそうな詳細化ステップ（候補）](#次にやるとよさそうな詳細化ステップ候補)
- [Kitu ライブラリ構成まとめ（crate / Unity パッケージ）](#kitu-ライブラリ構成まとめcrate--unity-パッケージ)
  - [Rust Workspace 全体構成（kitu リポジトリ）](#rust-workspace-全体構成kitu-リポジトリ)
  - [各 crate の責務](#各-crate-の責務)
  - [Unity 検証アプリ（kitu-integration-runner 配下）](#unity-検証アプリkitu-integration-runner-配下)
- [ユースケース一覧](#ユースケース一覧)
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
- [詳細フロー](#詳細フロー)



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

### Unity 検証アプリ（kitu-integration-runner 配下）

```
kitu-integration-runner/
  unity-app/
    .gitkeep
    (将来) Unity プロジェクト一式
      - Packages/
      - ProjectSettings/
      - Assets/
```

この Unity 検証アプリの役割:

- CI/CD で「アプリケーションが破壊されず動作すること」を継続検証する。
- `kitu-unity-ffi` と `kitu-runtime` の統合境界を検証する。
- 代表シナリオ（入力→tick更新→出力反映）を smoke テストとして実行する。
- ゲーム固有実装の母体ではなく、回帰検証用の最小アプリとして管理する。


このドキュメントでは、Kitu フレームワーク本体と、`kitu-integration-runner/unity-app` に配置する Unity 検証アプリのユースケース一覧を整理します。

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
