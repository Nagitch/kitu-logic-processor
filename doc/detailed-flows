# Kitu Detailed Flow Documentation

## TOC（詳細フローマップ）

- [UC-01: ゲーム起動 & シーン初期化](#uc-01-ゲーム起動--シーン初期化詳細フロー)
- [UC-02: メインループ（tick ごとのシミュレーション & 表示更新）](#uc-02-メインループtick-ごとのシミュレーション--表示更新)
- [UC-10: プレイヤー移動](#uc-10-プレイヤー移動詳細フロー)
- [UC-11: カメラ追従](#uc-11-カメラ追従詳細フロー)
- [UC-20: 敵スポーン](#uc-20-敵スポーン詳細フロー)
- [UC-21: プレイヤー近接攻撃](#uc-21-プレイヤー近接攻撃詳細フロー)
- [UC-22: 敵AI行動](#uc-22-敵ai行動詳細フロー)
- [UC-23: HP 減少 & 死亡処理](#uc-23-hp-減少--死亡処理詳細フロー)
- [UC-30: 経験値 & レベルアップ](#uc-30-経験値--レベルアップ詳細フロー)
- [UC-31: アイテム取得](#uc-31-アイテム取得詳細フロー)
- [UC-32: アイテム使用](#uc-32-アイテム使用詳細フロー)
- [UC-40: クエスト開始・進行・完了](#uc-40-クエスト開始進行完了詳細フロー)
- [UC-41: シナリオフラグ分岐](#uc-41-シナリオフラグ分岐詳細フロー)
- [UC-51: スキル発動演出（TSQ1）](#uc-51-スキル発動演出tsq1詳細フロー)
- [UC-60: HUD 更新](#uc-60-hud-更新詳細フロー)
- [UC-61: ポーズ--メニュー](#uc-61-ポーズ--メニュー詳細フロー)
- [UC-70: TMD ホットリロード](#uc-70-tmd-ホットリロード詳細フロー)
- [UC-72: Rhai スクリプト変更 → ホットリロード](#uc-72-rhai-スクリプト変更--ホットリロード詳細フロー)
- [UC-80: Kitu Shell からデバッグコマンド](#uc-80-kitu-shell-からデバッグコマンド詳細フロー)
- [UC-81: Web Admin で状態監視](#uc-81-web-admin-で状態監視詳細フロー)
- [UC-82: リプレイ（入力再生）](#uc-82-リプレイ入力再生詳細フロー)
- [UC-83: Web Admin から Kitu Shell コマンド実行](#uc-83-web-admin-から-kitu-shell-コマンド実行詳細フロー)
- [UC-90: セーブ・ロード](#uc-90-セーブロード詳細フロー)

---

このファイルでは Kitu を用いた各ユースケース（UC-01, UC-02 など）の詳細なアーキテクチャフローを個別に管理します。



---

## UC-01: ゲーム起動 & シーン初期化（詳細フロー）

### この UC で検証したいアーキテクチャ上のポイント

- Kitu フレームワークとゲーム固有コード（stella-rpg）の **リポジトリ分離 / crate 分離** が妥当かどうか。
- Unity ↔ Rust 間の **cdylib / FFI ブリッジの責務分離**（設定受け渡し・ライフサイクル管理）。
- 起動時に必要な **データロード / ECS 構築 / 初期イベント出力** のレイヤ分けが破綻なく行えるか。

### 0. 前提：コード構成

**Kitu リポジトリ（フレームワーク）**

- `kitu-runtime`, `kitu-ecs`, `kitu-osc-ir`
- `kitu-data-*`（TMD / SQLite）
- `kitu-tsq1`, `kitu-scripting-rhai`
- `kitu-unity-ffi`

**ゲームリポジトリ（stella-rpg）**

- Rust:
  - `game-core`（StellaGame 入口）
  - `game-data-schema`, `game-data-build`
  - `game-ecs-features`, `game-logic`, `game-scripts`, `game-timeline`
- Unity:
  - `com.kitu.runtime`（共通ブリッジ）
  - `com.stella.game`（タイトル固有の View 層）

以下は **Unity に cdylib を埋め込む構成**を前提に説明する。

---

### 1. Unity エディタで Play（初期化の開始）

Unity がシーンをロードし、`KituRuntimeBridge`（`com.kitu.runtime`）が `Awake()` / `Start()` で起動する。

Unity 側処理：

```csharp
void Start() {
    var configJson = BuildStellaConfigJson();
    KituNative.Initialize(configJson);
}
```

**ここで Rust 側に渡る情報**

- データフォルダパス
- tick レート
- ログ設定

**Rust 側で呼ばれる C API**

```
kitu-unity-ffi::kitu_initialize(config_json: *const c_char)
```

---

### 2. Rust 側：kitu\_initialize の内部処理

`kitu-unity-ffi` が担当し、次を行う：

1. JSON → `StellaConfig` にデコード
2. `StellaGame::new(config)` を呼ぶ
3. 生成したゲームインスタンスをグローバルに保持

```rust
#[no_mangle]
pub extern "C" fn kitu_initialize(config_json: *const c_char) -> KituHandle {
    let config_str = unsafe { CStr::from_ptr(config_json) }.to_string_lossy();
    let stella_config: StellaConfig = serde_json::from_str(&config_str).unwrap();
    let game = StellaGame::new(stella_config).unwrap();
    store_in_global_handle(game)
}
```

**関与 crate**

- `kitu-unity-ffi`
- `game-core`（`StellaGame::new`）

---

### 3. StellaGame::new（ゲームアプリ層の初期化）

```rust
pub fn new(config: StellaConfig) -> Result<Self, KituError> {
    // 1) Kitu Runtime を構築
    let mut runtime = KituRuntime::new(config.to_kitu_config())?;

    // 2) データロード（TMD / SQLite）
    let datastore = game_data_build::load_datastore(&config.data_root)?;

    // 3) ECS にゲーム固有コンポーネント & システム登録
    game_ecs_features::register_components(runtime.world_mut());
    game_ecs_features::register_systems(runtime.scheduler_mut());

    // 4) ゲームロジック/スクリプト/タイムライン初期化
    game_logic::attach_to_runtime(&mut runtime, &datastore)?;
    game_scripts::setup_rhai_api(&mut runtime, &datastore)?;
    game_timeline::setup_timelines(&mut runtime)?;

    Ok(Self { runtime })
}
```

**関与 crate**

- Kitu: `kitu-runtime`, `kitu-ecs`, `kitu-data-*`, `kitu-tsq1`, `kitu-scripting-rhai`
- ゲーム側: `game-core`, `game-data-build`, `game-data-schema`, `game-ecs-features`, `game-logic`, `game-scripts`, `game-timeline`

---

### 4. データロードとバリデーション（TMD / SQLite）

`game_data_build::load_datastore` が次を実行：

- `data/tmd/**/*.tmd` を読み込みパース（`kitu-data-tmd`）
- TMD → AST → SQLite または構造体に変換
- `game-data-schema` の型にマッピング（例：`UnitDef`, `ItemDef`）
- 参照整合性チェック（ID 重複、存在しない参照など）

**目的：** データ駆動部分の正当性確認。

---

### 5. 初期 ECS ワールド構築

`game-ecs-features` により：

- コンポーネント型の登録
- システム（movement, combat, ai, quest, ui など）の登録

これにより、**ECS 抽象（kitu-ecs）とゲーム固有ロジックが明確に分離**される。

---

### 6. 初期シーンの生成（バックエンド → Unity）

初回 tick または初期化直後に：

- プレイヤーエンティティ生成
- マップ初期化
- HUD 初期化

そして以下のようなイベントを出力キューへ積む：

```
/render/world/init
/render/player/spawn
/ui/hud/show
```

**関与 crate**

- `kitu-runtime`（出力イベント管理）
- `kitu-osc-ir`（`OscEvent` 型）
- ゲーム側ロジック（`game-logic`）

---

### 7. Unity が出力イベントを受信し、Scene を構築

`KituRuntimeBridge` が `Start()` または最初の `Update()` で：

1. `KituNative.PollEvents()` を呼び Rust 出力キューを取得
2. C# 側 `OscEvent` にデコード
3. `KituEventBus` に publish
4. `com.stella.game` の View が処理
   - プレイヤー GameObject の生成
   - 敵やオブジェクトの表示
   - HUD の表示

→ **Unity シーンが初期状態として完成する。**

---

## UC-02: メインループ（tick ごとのシミュレーション & 表示更新）

### この UC で検証したいアーキテクチャ上のポイント

- Unity のフレームレートと独立した **固定タイムステップの KituRuntime** を中核に据えられるか。
- 入力 → ECS → 出力イベント という **決定的なフェーズ分割** が、全ユースケースの土台として成立するか。
- Shell / WebAdmin / リプレイなど周辺機能が、同じメインループ上に **副作用なく統合** できるか。

### 全体概要

Unity の `Update()` から毎フレーム `deltaTime` が Rust 側に渡され、KituRuntime が tick ベース（例：60Hz）で ECS シミュレーションを進行する。判定・移動・AI・戦闘・死亡処理などのゲームロジックがバックエンドで完結し、結果として `/render/*`・`/ui/*` のイベントが Unity に返され、Unity は View 更新だけを行う。

---

### 1. Unity → Rust：deltaTime / 入力送信

Unity の毎フレームの更新処理：

```csharp
void Update(){
    var dt = Time.deltaTime;
    KituNative.Update(dt);     // Rust 側に deltaTime 送信
    SendInputIfAny();          // 入力があれば /input/* を送信
}
```

**責務（Unity 側）**

- 時間経過の通知（deltaTime）
- 入力イベントの送信（例：/input/move, /input/attack）
- ゲームロジックは保持しない（純粋な入出力）

**Rust 側で呼ばれる API**

```
kitu-unity-ffi::kitu_update(handle, delta_seconds: f32)
```

---

### 2. Rust: KituRuntime.update(dt) の中心処理

Rust 側では `KituRuntime.update(dt)` がメインループの中心を担う。

```rust
pub fn update(&mut self, dt: f32) {
    self.time.accumulate(dt);

    while self.time.should_step() {
        self.step_one_tick();
    }
}
```

**役割**

- deltaTime を Accumulator に追加
- 必要 tick 数分だけ ECS システムを実行
- 固定時間ステップであるため、Unity のフレームレートと分離される（60Hz など）

---

### 3. 1 tick の ECS スケジューリング

1 tick ごとに、以下のフェーズを順序正しく実行する。 ゲームの決定性（determinism）を担保するため、順番は固定。

#### フェーズ 1：入力処理

- `/input/move` `/input/attack` などを ECS に反映
- ゲームエンティティのステートを更新（Velocity, ActionState など）

#### フェーズ 2：AI / スクリプト実行

- 敵 AI の行動決定（移動・攻撃など）
- Rhai スクリプトで書かれたクエスト進行ロジックの実行

#### フェーズ 3：物理 / 移動

- Velocity を Position に反映
- コリジョン判定（簡易）

#### フェーズ 4：戦闘 / ダメージ計算

- 当たり判定
- スキル効果
- HP 数値の更新

#### フェーズ 5：死亡処理

- HP <= 0 のエンティティを死亡状態に
- デスポーン処理

#### フェーズ 6：レンダリング用データ収集

- Transform の収集
- UI 表示情報の収集（HP・ステータス・HUD）
- Unity 側へ送るイベントをキューへ積む

**関与 crate**

- `kitu-ecs`
- `game-ecs-features`
- `game-logic`
- `kitu-tsq1`（スキル演出があれば）
- `kitu-runtime`（イベント管理）

---

### 4. `/render/*` `/ui/*` `/debug/*` の出力イベント生成

tick 処理の結果、次のようなイベントが出力される：

```
/render/player/transform
/render/enemy/transform
/render/enemy/dead
/ui/hud/update
/debug/log
```

**イベント生成のポイント**

- Unity への Event はすべて OSC-IR (`kitu-osc-ir`) 形式
- 出力イベントは **KituRuntime の出力キューに蓄積**
- PollEvents で Unity に返る

---

### 5. Unity が PollEvents で結果を取得 → View 更新

Unity 側では、毎フレーム `PollEvents()` を呼んで Rust の出力キューをまとめて受け取る。

```csharp
var events = KituNative.PollEvents();
foreach (var ev in events) {
    KituEventBus.Publish(ev);
}
```

**Unity View 層（com.stella.game）** が処理：

- プレイヤー位置を Transform に反映
- 敵の出現・死亡アニメーション
- HUD（HP・MP・経験値バーなど）を更新

※ Unity はゲームロジックを持たず、純粋に表示のみ行うのが Kitu の基本設計である。

---

### 6. Shell / WebAdmin / リプレイの統合（概略）

#### Shell（kitu-shell / game-shell-ext）

`spawn_enemy goblin` などのコマンドは `/debug/*` イベントとして Runtime に流れ、tick 内で処理される。

#### Web Admin（kitu-web-admin-backend / game-webadmin-ext）

WebSocket で Runtime と接続し、以下を取得：

- ECS 状態（敵/プレイヤーの位置、HP）
- ログ
- デバッグイベント結果

#### リプレイ（kitu-replay-runner）

- 入力イベントログを tick 単位で再生し、決定的に同じ結果を再現可能

これらは **すべて通常の ********\`\`******** tick と同じパス**を通るため、挙動の一貫性が保たれる。

---

## UC-10: プレイヤー移動（詳細フロー）

### この UC で検証したいアーキテクチャ上のポイント

- 入力解釈は Unity、移動ロジックは Rust という **入力とシミュレーションの責務分離** が自然に書けるか。
- `/input/move` → ECS → `/render/player/transform` の **イベントパイプラインがシンプルな定型パターン** として再利用できるか。
- Kitu 側の ECS 抽象で、将来のコリジョンや地形制約も拡張しやすいか。

### 全体概要

プレイヤーの移動入力（WASD / 左スティック）を **Unity → KituRuntime → ECS** に伝え、 バックエンド側で決定的にポジション更新を行い、その結果を `/render/player/transform` として Unity に送り返す。

- 入力解釈は Unity 側
- 移動量の計算と状態管理は Rust（Kitu + game-\*）側
- Unity は Transform 反映のみを担当

---

### 1. Unity 側：入力取得と `/input/move` イベント送信

**関与レイヤー**

- Unity パッケージ: `com.kitu.runtime`
- C# 側 API: `KituNative.SendInputMove`

```csharp
var axis = new Vector2(Input.GetAxis("Horizontal"), Input.GetAxis("Vertical"));
if (axis.sqrMagnitude > 0.0f)
{
    KituNative.SendInputMove(axis.x, axis.y);
}
```

送信されるイベント（論理表現）：

```text
/address: "/input/move"
args: { x: 0.5, y: 1.0 }
```

---

### 2. Rust 側：入力イベントを取り込み、キューに保存

**関与 crate**

- `kitu-unity-ffi`（C API → `OscEvent` 変換）
- `kitu-runtime`（入力イベントキュー管理）
- `kitu-osc-ir`（`OscEvent` 型）

フロー：

1. `kitu_unity_ffi::kitu_send_input_move(x, y)` が呼ばれる
2. `OscEvent { address: "/input/move", args: [x, y] }` を構築
3. `KituRuntime::enqueue_input(event)` で入力キューに積む

この時点ではまだ ECS には反映されず、**次の tick フェーズ 1 で処理される**。

---

### 3. ECS フェーズ 1：入力処理で Velocity を更新

**関与 crate**

- `kitu-ecs`
- `game-ecs-features`（`InputMove`, `Velocity` コンポーネント）
- `game-logic`（移動速度定数など）

入力システム例：

```rust
fn input_movement_system(world: &mut World) {
    let move_input = world.resource::<InputMoveState>();

    for (_player, mut velocity) in world.query_mut::<(&PlayerTag, &mut Velocity)>() {
        velocity.x = move_input.x * MOVE_SPEED;
        velocity.y = move_input.y * MOVE_SPEED;
    }
}
```

- `/input/move` は一旦 `InputMoveState` リソースに反映
- その値を元にプレイヤーの `Velocity` を更新

---

### 4. ECS フェーズ 3：移動システムで Position を更新

```rust
fn movement_system(world: &mut World) {
    for (_player, mut pos, vel) in world.query_mut::<(&PlayerTag, &mut Position, &Velocity)>() {
        pos.x += vel.x;
        pos.y += vel.y;
    }
}
```

- tick ごとに 1 ステップ進む
- ここには簡易コリジョンや地形制約も後から追加可能

---

### 5. ECS フェーズ 6：レンダリング情報の収集と `/render/player/transform`

**関与 crate**

- `kitu-runtime`
- `kitu-osc-ir`

収集システム例：

```rust
fn gather_player_render_events(world: &World, out_events: &mut Vec<OscEvent>) {
    for (_player, pos) in world.query::<(&PlayerTag, &Position)>() {
        out_events.push(OscEvent::render_player_transform(1, pos));
    }
}
```

生成されるイベント：

```text
/render/player/transform {
  id: 1,
  position: { x: 10.0, y: 0.0, z: 5.0 }
}
```

---

### 6. Unity View 側で Transform を反映

**関与レイヤー**

- Unity: `com.kitu.runtime`（イベントバス）
- Unity: \`com.st
