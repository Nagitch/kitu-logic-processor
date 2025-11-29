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

### 2. Rust 側：kitu_initialize の内部処理

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

これらは **すべて通常の `KituRuntime` tick と同じパス**を通るため、挙動の一貫性が保たれる。

---

## UC-10: プレイヤー移動（詳細フロー）

### この UC で検証したいアーキテクチャ上のポイント

- 入力解釈は Unity、移動ロジックは Rust という **入力とシミュレーションの責務分離** が自然に書けるか。
- `/input/move` → ECS → `/render/player/transform` の **イベントパイプラインがシンプルな定型パターン** として再利用できるか。
- Kitu 側の ECS 抽象で、将来のコリジョンや地形制約も拡張しやすいか。

### 全体概要

プレイヤーの移動入力（WASD / 左スティック）を **Unity → KituRuntime → ECS** に伝え、 バックエンド側で決定的にポジション更新を行い、その結果を `/render/player/transform` として Unity に送り返す。

- 入力解釈は Unity 側
- 移動量の計算と状態管理は Rust（Kitu + game-*）側
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
- Unity: `com.stella.game`（`PlayerView`）

```csharp
public class PlayerView : MonoBehaviour
{
    void OnEnable() {
        KituEventBus.Subscribe("/render/player/transform", OnTransformEvent);
    }

    void OnTransformEvent(OscEvent ev) {
        var pos = ev.ReadVector3("position");
        transform.position = pos;
    }
}
```

Unity は **Transform を更新するだけ** で、移動ロジックは一切持たないことを再確認できる。

---

## UC-11: カメラ追従（詳細フロー）

### この UC で検証したいアーキテクチャ上のポイント

- カメラの挙動を **完全に Unity 側の責務** とし、Kitu は位置情報のみ提供する構造が維持できるか。
- 「見せ方」の変更（カメラワーク / ポストエフェクト）が、Rust 側のロジックに影響を与えないことを確認する。
- Transform 同期の仕組みが、カメラ以外の View コンポーネントにも一貫して適用できるか。

### 全体概要

プレイヤーの位置に基づいてカメラを追従させる処理を **Unity 側だけで完結させる** ユースケース。 Kitu はプレイヤーの Transform を通知するだけで、カメラ制御の責務は持たない。

---

### 1. プレイヤー Transform イベントの購読

**関与レイヤー**

- Unity: `com.kitu.runtime`（イベントバス）
- Unity: `com.stella.game`（`CameraFollow` コンポーネント）

```csharp
public class CameraFollow : MonoBehaviour
{
    public Transform player;
    public Vector3 offset;

    void LateUpdate()
    {
        if (player != null)
        {
            transform.position = player.position + offset;
        }
    }
}
```

- `PlayerView` 側でプレイヤー Transform を更新
- `CameraFollow` は常にその Transform を参照して位置を更新

---

### 2. 責務分離の確認ポイント

- Kitu（Rust）は **「プレイヤーがどこにいるか」** までを担当
- Unity（C#）は **「その結果をどう見せるか」** を担当
- カメラ制御を Rust 側に持たないことで、
  - 表現の変更（カメラワーク）を Unity 側だけで差し替え可能
  - バックエンドのシミュレーション決定性がシンプルに保たれる

---

## UC-20: 敵スポーン（詳細フロー）

### この UC で検証したいアーキテクチャ上のポイント

- Rhai / TSQ1 / Shell など **複数のトリガー源を、同一の spawn API に集約** できるか。
- 敵の内部表現（ECS コンポーネント）と Unity の prefab / View を、`id` と `prefabKey` で **疎結合に結びつける設計** が妥当か。
- スポーン検出〜`/render/enemy/spawn` 出力までを、他のエンティティ種別にも汎用化しやすいか。

### 全体概要

敵を生成するトリガー（Rhai スクリプト、TSQ1、Shell コマンドなど）から、 **ECS 内に敵エンティティを追加し、その結果を `/render/enemy/spawn` として Unity に通知する** までの流れ。

- 「どの敵を、どこに、どのような状態で出すか」は Rust 側（game-logic）が決定
- Unity は「どの prefab を Instantiate するか」を知っていればよい

---

### 1. スポーントリガーの発火

トリガーの例：

- Rhai スクリプト：`spawn_enemy("goblin", 10.0, 5.0);`
- TSQ1 イベント：`/game/spawn_enemy goblin 10 5`
- Shell コマンド：`spawn_enemy goblin 10 5`

**関与レイヤー**

- `game-scripts`（Rhai バインディング）
- `kitu-tsq1` / `game-timeline`（TSQ1）
- `kitu-shell` / `game-shell-ext`（Shell）

どの場合も最終的には **同じ Rust 関数** に集約される：

```rust
fn spawn_enemy_api(kind: EnemyKind, pos: Vec2, world: &mut World) {
    game_logic::spawn_enemy(kind, pos, world);
}
```

---

### 2. ECS 内で敵エンティティ生成

**関与 crate**

- `kitu-ecs`
- `game-ecs-features`（`EnemyTag`, `Hp`, `Position`, `EnemyKind` など）
- `game-logic`（敵の初期ステータス決定）

```rust
pub fn spawn_enemy(kind: EnemyKind, pos: Vec2, world: &mut World) {
    let stats = EnemyStats::from_kind(kind);

    world.spawn((
        EnemyTag,
        kind,
        Position { x: pos.x, y: 0.0, z: pos.y },
        Velocity::zero(),
        Hp::new(stats.max_hp),
        AttackPower(stats.atk),
        EnemyState::Idle,
    ));
}
```

ここで **ゲームとして必要な全コンポーネント** を ECS に登録する。

---

### 3. `/render/enemy/spawn` イベント生成

敵エンティティが生成されたタイミング、あるいは専用の「スポーン検出システム」により、 Unity へ通知するためのイベントを生成する。

```rust
fn gather_enemy_spawn_events(world: &World, out_events: &mut Vec<OscEvent>) {
    for (entity, enemy, pos) in world.query::<(Entity, &EnemyTag, &Position)>() {
        if just_spawned(entity) {
            out_events.push(OscEvent::render_enemy_spawn(entity, enemy.kind, pos));
        }
    }
}
```

生成されるイベント例：

```text
/render/enemy/spawn {
  id: 42,
  kind: "goblin",
  position: { x: 10.0, y: 0.0, z: 5.0 },
  prefab: "Enemies/Goblin"
}
```

**関与 crate**

- `kitu-runtime`（出力イベントキュー）
- `kitu-osc-ir`（イベント型）
- `game-logic`（prefab 名、kind などの決定）

---

### 4. Unity 側：GameObject の生成

**関与レイヤー**

- Unity: `com.kitu.runtime`（イベントバス）
- Unity: `com.stella.game`（`EnemySpawnerView`）

```csharp
public class EnemySpawnerView : MonoBehaviour
{
    [SerializeField] EnemyPrefabRegistry _registry;

    void OnEnable() {
        KituEventBus.Subscribe("/render/enemy/spawn", OnSpawnEvent);
    }

    void OnSpawnEvent(OscEvent ev) {
        var id = ev.ReadInt("id");
        var prefabKey = ev.ReadString("prefab");
        var pos = ev.ReadVector3("position");

        var prefab = _registry.Get(prefabKey);
        var go = Instantiate(prefab, pos, Quaternion.identity);
        go.GetComponent<EnemyView>().BindEntityId(id);
    }
}
```

Unity では：

- **どの敵を**：`prefabKey` / `kind`
- **どこに**：`position`
- **どの ECS エンティティに対応するか**：`id`

を受け取り、見た目の生成のみ行う。

---

## UC-21: プレイヤー近接攻撃（詳細フロー）

### この UC で検証したいアーキテクチャ上のポイント

- 攻撃ボタン入力からヒット判定・ダメージ計算・死亡判定までを **完全に Rust 側で完結** させられるか。
- ヒット結果を `/render/*` イベントとして表現することで、Unity 側の演出を **純粋な View として差し替え可能** にできるか。
- 戦闘ロジックが他の入出力（ネットワーク / リプレイ）とも整合的に動くか。

### 全体概要

プレイヤーの攻撃ボタン入力から、**ヒット判定・ダメージ計算・HP 更新・ヒット演出** までを一貫して Rust 側で行い、Unity はアニメーションとエフェクトを担当する。

---

### 1. Unity → `/input/attack` 送信

**関与レイヤー**

- Unity: `com.kitu.runtime`

```csharp
if (Input.GetButtonDown("Fire1"))
{
    KituNative.SendInputAttack();
}
```

Rust 側には：

```text
/address: "/input/attack"
args: {}
```

が送られる。

---

### 2. ECS フェーズ 1：攻撃ステートの付与

**関与 crate**

- `kitu-ecs`
- `game-ecs-features`（`AttackRequest`, `ActionState` など）

```rust
fn input_attack_system(world: &mut World) {
    if world.resource::<InputAttackState>().pressed {
        for (_player, mut state) in world.query_mut::<(&PlayerTag, &mut ActionState)>() {
            state.request_attack();
        }
    }
}
```

- `/input/attack` はリソースに入り、プレイヤーに「攻撃したい」というフラグを付与

---

### 3. ECS フェーズ 4：戦闘システムでヒット判定・ダメージ計算

**関与 crate**

- `game-logic`（戦闘ルール）

```rust
fn melee_combat_system(world: &mut World, events: &mut Vec<OscEvent>) {
    let player_pos = /* プレイヤー位置 */;

    for (enemy_entity, enemy_pos, mut hp) in world.query_mut::<(Entity, &Position, &mut Hp)>().with::<EnemyTag>() {
        if in_attack_range(player_pos, enemy_pos) {
            let damage = calc_damage(/* 攻撃力など */);
            hp.current -= damage;

            events.push(OscEvent::enemy_hit(enemy_entity, damage));
        }
    }
}
```

- 当たり判定とダメージ計算はすべて Rust 側で行う
- ヒット結果は `/render/enemy/hit` のようなイベントとして Unity に通知

---

### 4. HP 更新と死亡判定（UC-23 への橋渡し）

HP が 0 以下になった場合：

- `EnemyState::Dead` に遷移
- UC-23（死亡処理）で `/render/enemy/dead` を出力

これにより、**攻撃 → ダメージ → 死亡演出** の流れがつながる。

---

### 5. Unity 側：アニメーション・エフェクト再生

**関与レイヤー**

- Unity: `EnemyView`

```csharp
void OnHit(OscEvent ev) {
    int id = ev.ReadInt("id");
    int damage = ev.ReadInt("damage");
    if (id != _entityId) return;

    _animator.SetTrigger("Hit");
    SpawnHitEffect();
}
```

- 実際のアニメーションやパーティクルは Unity 側でのみ管理

---

## UC-22: 敵AI行動（詳細フロー）

### この UC で検証したいアーキテクチャ上のポイント

- 敵 AI を ECS システムとして実装し、**状態遷移と意思決定をデータ駆動** で表現できるか。
- AI の出力が通常の移動・戦闘パイプライン（UC-10 / 21）を **再利用するだけの構造** にできているか。
- 将来的な AI バリエーション追加時に、Unity 側のコード変更を最小限に抑えられるか。

### 全体概要

敵 AI 는 tick ごとに Rust 側の ECS システムとして実行され、プレイヤーの位置等に基づいて移動や攻撃を決める。 Unity は結果としての Transform/アニメーションを受け取るだけ。

---

### 1. AI システムの登録

**関与 crate**

- `game-ecs-features`（`EnemyAiState` など）
- `game-logic`（AI ルール）

```rust
fn enemy_ai_system(world: &mut World) {
    let player_pos = /* プレイヤー位置 */;

    for (_enemy, mut ai, mut vel, pos) in world.query_mut::<(&EnemyTag, &mut EnemyAiState, &mut Velocity, &Position)>() {
        match ai.behavior {
            Behavior::Idle => {
                if distance(pos, player_pos) < ai.detect_radius {
                    ai.behavior = Behavior::Chase;
                }
            }
            Behavior::Chase => {
                vel.x = (player_pos.x - pos.x).signum() * ai.move_speed;
                vel.z = (player_pos.z - pos.z).signum() * ai.move_speed;
            }
        }
    }
}
```

- AI の状態遷移と Velocity の決定は Rust 側

---

### 2. 攻撃タイミングの決定と `/combat/attack` 相当の内部イベント

プレイヤーとの距離が一定以下になった場合など：

```rust
if distance(pos, player_pos) < ai.attack_range {
    queue_enemy_attack(enemy_entity);
}
```

- 実際の攻撃処理は UC-21 の戦闘システムに委譲
- AI は「攻撃したい」という意思決定のみ担当

---

### 3. 移動結果は UC-02 / UC-10 と同じパイプラインで Transform に反映

- Velocity → Position 更新（movement_system）
- `/render/enemy/transform` イベント生成
- Unity 側 `EnemyView` が Transform を更新

AI 専用の特別なパスはなく、通常の移動・レンダリングパイプラインを共有する。

---

## UC-23: HP 減少 & 死亡処理（詳細フロー）

### この UC で検証したいアーキテクチャ上のポイント

- HP と生死状態を ECS コンポーネントとして扱い、**敵 / プレイヤー共通の死亡パイプライン** を構成できるか。
- 実際のアニメーション / エフェクトは Unity 側に任せつつ、「死んだ」という事実だけを `/render/*` で通知する設計が機能するか。
- デスポーン処理（ECS エンティティ削除）と View 破棄の同期を、イベントで安全に表現できるか。

### 全体概要

敵やプレイヤーの HP が 0 以下になったときの **死亡処理・デスポーン・演出** を Rust 側で完結させ、Unity には `/render/*` イベントで通知する。

---

### 1. HP 監視システムで死亡条件を検知

**関与 crate**

- `kitu-ecs`
- `game-ecs-features`（`Hp`, `EnemyTag`, `PlayerTag`）

```rust
fn death_check_system(world: &mut World, events: &mut Vec<OscEvent>) {
    for (entity, hp) in world.query::<(Entity, &Hp)>() {
        if !hp.is_dead() { continue; }

        if world.entity_has::<EnemyTag>(entity) {
            events.push(OscEvent::enemy_dead(entity));
            mark_for_despawn(entity);
        } else if world.entity_has::<PlayerTag>(entity) {
            events.push(OscEvent::player_dead(entity));
        }
    }
}
```

---

### 2. `/render/enemy/dead` イベントとデスポーン

敵死亡時に出力されるイベント例：

```text
/render/enemy/dead { id: 42 }
```

- Runtime 側では `mark_for_despawn` などの仕組みで「次のフェーズで削除すべきエンティティ」として登録
- 別システムで実際の ECS エンティティ削除を行う

---

### 3. Unity 側：デスアニメーション → GameObject 破棄

```csharp
public class EnemyView : MonoBehaviour
{
    void OnEnable() {
        KituEventBus.Subscribe("/render/enemy/dead", OnDeadEvent);
    }

    void OnDeadEvent(OscEvent ev) {
        if (ev.ReadInt("id") != _entityId) return;

        _animator.SetTrigger("Dead");
        StartCoroutine(Co_Die());
    }

    IEnumerator Co_Die(){
        yield return new WaitForSeconds(1.0f);
        Destroy(gameObject);
    }
}
```

- 実際のアニメーションタイミングや残骸処理は Unity 側にのみ存在
- Rust 側は「死んだ」という真実だけを伝える

---

## UC-30: 経験値 & レベルアップ（詳細フロー）

### この UC で検証したいアーキテクチャ上のポイント

- XP・レベル・ステータス成長を **データ駆動 + game-logic 側の関数** で一元管理できるか。
- レベルアップ通知を `/ui/*` に限定することで、Unity の演出変更を **ゲームロジック非依存** に保てるか。
- 将来のバランス調整において、TMD/SQLite 変更と game-logic だけで完結させられるか。

### 全体概要

敵の撃破などにより取得した経験値（XP）を、プレイヤーのステータスに反映し、 レベルアップ時にはステータス上昇と UI 演出を行う。

- XP の加算・レベルの計算は Rust 側（game-logic）
- Unity は `/ui/levelup` や `/ui/hud/update` を受けて見た目を更新するだけ

---

### 1. 敵死亡時に XP を加算

**関与 crate**

- `game-logic`（ドロップ・XP テーブル）
- `game-ecs-features`（`Xp`, `Level`, `PlayerTag`）

敵死亡処理（UC-23）内、あるいはその直後のシステムで XP を加算：

```rust
fn grant_xp_on_enemy_death(world: &mut World) {
    for (enemy, xp_reward) in world.query::<(&EnemyTag, &XpReward)>() {
        if enemy.just_died() {
            let reward = xp_reward.0;
            for (_player, mut xp) in world.query_mut::<(&PlayerTag, &mut Xp)>() {
                xp.current += reward;
            }
        }
    }
}
```

- `XpReward` は `game-data-schema` + `game-data-build` 由来
- 複数プレイヤー対応も拡張可能

---

### 2. レベルアップ判定とステータス更新

**関与 crate**

- `game-logic`（レベルカーブ）
- `game-data-schema`（テーブル定義）

```rust
fn level_up_system(world: &mut World, events: &mut Vec<OscEvent>) {
    for (_player, mut xp, mut level, mut stats) in world
        .query_mut::<(&PlayerTag, &mut Xp, &mut Level, &mut Stats)>()
    {
        while xp.current >= xp_to_next(level.value) {
            xp.current -= xp_to_next(level.value);
            level.value += 1;

            let inc = stats_growth_for_level(level.value);
            stats.max_hp += inc.hp;
            stats.atk += inc.atk;

            events.push(OscEvent::ui_levelup(level.value));
        }
    }
}
```

- `xp_to_next` や `stats_growth_for_level` は `game-logic` 内の関数
- レベルアップが発生すると `/ui/levelup` イベントを出力

---

### 3. Unity 側：レベルアップ UI 演出

**関与レイヤー**

- Unity: `LevelUpView`（`com.stella.game`）

```csharp
public class LevelUpView : MonoBehaviour
{
    void OnEnable() {
        KituEventBus.Subscribe("/ui/levelup", OnLevelUp);
    }

    void OnLevelUp(OscEvent ev) {
        int level = ev.ReadInt("level");
        _popup.Show($"Level {level}!");
        _animator.SetTrigger("LevelUp");
    }
}
```

- HUD のステータス表示更新は、別途 `/ui/hud/update` で行う

---

## UC-31: アイテム取得（詳細フロー）

### この UC で検証したいアーキテクチャ上のポイント

- ドロップテーブル〜インベントリ更新までを **Rust 側で完結するドメインロジック** として扱えるか。
- インベントリ UI を `/ui/inventory/update` のみで駆動し、Unity 側を **完全な表示レイヤ** として保てるか。
- アイテム種別の追加が、TMD/SQLite + game-logic 変更だけで済む構造になっているか。

### 全体概要

敵のドロップや宝箱からアイテムを取得し、インベントリに追加して UI に反映する。

- 何をどれだけ入手するかは Rust 側（game-logic）が決定
- Unity はインベントリ UI の表示のみ担当

---

### 1. ドロップテーブルの評価

**関与 crate**

- `game-data-schema`（`DropTableDef` など）
- `game-data-build`（TMD/SQLite からのロード）
- `game-logic`（乱数・テーブル解決）

```rust
fn roll_drop_for_enemy(enemy_kind: EnemyKind, rng: &mut Rng) -> Vec<ItemStack> {
    let table = drop_table_for(enemy_kind);
    table.roll(rng)
}
```

敵死亡時（UC-23 のフロー内）で呼び出す：

```rust
let drops = roll_drop_for_enemy(enemy.kind, &mut rng);
for stack in drops {
    add_item_to_inventory(player_id, stack, &mut world);
}
```

---

### 2. インベントリへの追加

**関与 crate**

- `game-ecs-features`（`Inventory` コンポーネント）
- `game-logic`（スタック処理）

```rust
fn add_item_to_inventory(player: Entity, stack: ItemStack, world: &mut World) {
    let mut inv = world.get_mut::<Inventory>(player).unwrap();
    inv.add(stack);
}
```

追加後、UI 通知用イベントを生成：

```rust
events.push(OscEvent::ui_inventory_update(player, &inv));
```

---

### 3. Unity 側：インベントリ UI の更新

**関与レイヤー**

- Unity: `InventoryView`

```csharp
void OnInventoryUpdate(OscEvent ev) {
    var items = DecodeItems(ev);
    _grid.Bind(items);
}
```

- 実際のレイアウトやアイコン画像の管理は Unity 側

---

## UC-32: アイテム使用（詳細フロー）

### この UC で検証したいアーキテクチャ上のポイント

- アイテム使用のトリガーを `/input/item/use` に統一し、効果適用を **game-logic 内のテーブル駆動** で表現できるか。
- HUD / インベントリ更新をすべて `/ui/*` イベント経由で行い、副作用を View 層に閉じ込められるか。
- バフ / デバフなど複雑な効果を追加しても、パイプラインを崩さず拡張できるか。

### 全体概要

プレイヤーがインベントリ UI からアイテムを選択し、使用効果（回復など）を Rust 側で適用し、その結果を HUD/UI に反映する。

---

### 1. Unity → `/input/item/use` イベント送信

**関与レイヤー**

- Unity: `InventoryView`

```csharp
public void OnUseButtonClicked(ItemUiModel item) {
    KituNative.SendUseItem(item.ItemId);
}
```

送信されるイベント例：

```text
/address: "/input/item/use"
args: { item_id: 1001 }
```

---

### 2. Rust 側：アイテム使用ロジック

**関与 crate**

- `game-logic`（アイテム効果テーブル）
- `game-data-schema`（`ItemDef`）

```rust
fn use_item_system(world: &mut World, events: &mut Vec<OscEvent>) {
    let use_req = world.resource::<ItemUseRequest>();
    if let Some(item_id) = use_req.take() {
        let player = find_player_entity(world);
        apply_item_effect(player, item_id, world, events);
    }
}

fn apply_item_effect(player: Entity, item_id: ItemId, world: &mut World, events: &mut Vec<OscEvent>) {
    let def = item_def(item_id);

    match def.effect {
        ItemEffect::Heal(hp) => {
            let mut stats = world.get_mut::<Stats>(player).unwrap();
            stats.hp = (stats.hp + hp).min(stats.max_hp);
            events.push(OscEvent::ui_hud_update(&stats));
        }
        ItemEffect::Buff(..) => { /* 省略 */ }
    }

    // インベントリから消費
    let mut inv = world.get_mut::<Inventory>(player).unwrap();
    inv.consume(item_id, 1);
    events.push(OscEvent::ui_inventory_update(player, &inv));
}
```

---

### 3. Unity 側：HUD / インベントリ UI 更新

- `/ui/hud/update` → HP バーなどのステータス表示を更新
- `/ui/inventory/update` → アイテム残数を更新

```csharp
void OnHudUpdate(OscEvent ev) {
    var hp = ev.ReadInt("hp");
    var maxHp = ev.ReadInt("max_hp");
    _hpBar.Set(hp, maxHp);
}
```

Unity は「見せ方」だけをコントロールし、 **どのタイミングで何が起こるかは Rust 側が完全に決める** という構造が維持される。

---

## UC-40: クエスト開始・進行・完了（詳細フロー）

### この UC で検証したいアーキテクチャ上のポイント

- クエスト状態を `QuestLog` リソースとして集中管理し、**複数システムからの更新を一元化** できるか。
- Rhai / TSQ1 からクエスト API を呼ぶだけで、進行・報酬処理が **同じバックエンドロジック** に集約されるか。
- クエスト UI を `/ui/quest/*` イベントで完全に駆動し、表示と状態遷移を分離できるか。

### 全体概要

クエストの開始・進行・完了と、それに紐づく UI 表示やシナリオの解放を、 **Rust 側のクエストシステム + Rhai スクリプト** を中心に実現する。

- クエスト状態（未開始 / 進行中 / 完了 / 報酬受取済）は ECS / リソース側で管理
- Rhai スクリプトや TSQ1 から「クエスト開始 / 進行」API を呼び出す
- Unity は `/ui/quest/update` などのイベントを受けてリスト表示を更新するのみ

---

### 1. クエスト開始：Rhai スクリプトからの呼び出し

**関与 crate**

- `game-scripts`（Rhai バインディング）
- `game-logic`（クエストロジック）

Rhai スクリプト例：

```rhai
// 会話イベント中など
fn on_talk_with_npc() {
    quest_start("find_lost_sword");
}
```

Rust 側 API：

```rust
pub fn quest_start(id: QuestId, world: &mut World) {
    let mut log = world.resource_mut::<QuestLog>();
    log.start(id);
}
```

`QuestLog` リソースは、`HashMap<QuestId, QuestState>` などで管理。

---

### 2. クエスト進行条件のチェック（ECS システム）

**関与 crate**

- `game-ecs-features`（`QuestProgress`, `QuestRelated` コンポーネント）
- `game-logic`

例：特定アイテム入手、敵撃破数などを集計：

```rust
fn quest_progress_system(world: &mut World, events: &mut Vec<OscEvent>) {
    let mut log = world.resource_mut::<QuestLog>();

    for (entity, progress) in world.query::<(Entity, &QuestRelated)>() {
        log.update_from_entity(entity, progress);
    }

    // 状態に変化があれば UI 更新イベント
    if log.consume_dirty_flag() {
        events.push(OscEvent::ui_quest_update(&log));
    }
}
```

- クエスト状態に変化があった tick で `/ui/quest/update` を出力

---

### 3. クエスト完了・報酬処理

条件を満たしたクエストは `QuestState::Completed` へ：

```rust
if log.is_completed(qid) {
    // 報酬付与
    grant_rewards(qid, world, events);
}
```

報酬は：

- XP やアイテム（UC-30, 31 と連携）
- シナリオフラグ（UC-41 と連携）

として加算し、必要に応じて `/ui/quest/complete` などのイベントを Unity に通知。

---

### 4. Unity 側：クエストログ UI 更新

**関与レイヤー**

- Unity: `QuestLogView`

```csharp
void OnQuestUpdate(OscEvent ev) {
    var quests = DecodeQuestLog(ev);
    _list.Bind(quests); // タイトル、進行度、完了フラグなど
}
```

Unity 側は「どのクエストがどの状態か」を表示するだけで、 **状態遷移のロジック自体はすべて Rust 側で完結させる**。

---

## UC-41: シナリオフラグ分岐（詳細フロー）

### この UC で検証したいアーキテクチャ上のポイント

- シナリオ上の分岐条件を `ScenarioFlags` に集約し、**ゲーム全体で使い回せる真理値ストア** として機能させられるか。
- Rhai / TSQ1 / Shell から同じフラグ API を利用することで、分岐ロジックを一箇所に閉じ込められるか。
- リプレイやデバッグ時に、フラグ状態を再現するだけで分岐挙動を完全に再現できるか。

### 全体概要

ゲーム中の分岐条件（NPC の反応、イベント解放など）を、 **`ScenarioFlags` リソース + Rhai スクリプト** で一貫管理する。

- 「フラグを立てる・消す」は Rust 側 API 経由
- Rhai スクリプトや TSQ1 からは `set_flag()/is_flag_set()` などで参照
- Unity は基本的にフラグを意識せず、Rhai によって決まった結果を表示するだけ

---

### 1. フラグ管理リソース

**関与 crate**

- `game-logic`

```rust
pub struct ScenarioFlags {
    flags: HashSet<FlagId>,
}

impl ScenarioFlags {
    pub fn set(&mut self, id: FlagId) { self.flags.insert(id); }
    pub fn unset(&mut self, id: FlagId) { self.flags.remove(&id); }
    pub fn is_set(&self, id: FlagId) -> bool { self.flags.contains(&id) }
}
```

---

### 2. Rhai からのフラグ操作 API

**関与 crate**

- `game-scripts`（Rhai 統合）

```rust
fn bind_rhai(engine: &mut rhai::Engine, world: &mut World) {
    engine.register_fn("set_flag", move |id: String| {
        world.resource_mut::<ScenarioFlags>().set(FlagId::from(id));
    });

    engine.register_fn("is_flag_set", move |id: String| -> bool {
        world.resource::<ScenarioFlags>().is_set(FlagId::from(id))
    });
}
```

Rhai スクリプト例：

```rhai
if is_flag_set("quest_sword_completed") {
    show_dialog("ありがとう、剣を見つけてくれたんだね！");
} else {
    show_dialog("迷子の剣を探してきてくれないか？");
}
```

---

### 3. TSQ1 からのフラグ参照

TSQ1 イベント内で、条件分岐的にフラグを参照できるような設計も可能：

- `/scenario/branch if flag=xxx`
- あるいは「TSQ1 → Rhai 呼び出し」で分岐を外部化

詳細な仕様は TSQ1 側の定義に委ねるが、 **決定的な分岐条件として ScenarioFlags を利用する**ことで、 リプレイやデバッグ時にも同じ挙動が再現できる。

---

## UC-51: スキル発動演出（TSQ1）（詳細フロー）

### この UC で検証したいアーキテクチャ上のポイント

- 戦闘ロジック（ダメージ計算）と演出ロジック（エフェクト・SE・カメラ）を、**TSQ1 タイムラインで明確に分離** できるか。
- 時間軸に沿った演出を TSQ1 側に記述し、KituRuntime の tick と自然に同期させられるか。
- Unity 側の Effect / Camera / Sound コンポーネントを、TSQ1 からの `/render/*` / `/sound/*` イベントだけで駆動できるか。

### 全体概要

スキル発動時に、**TSQ1 タイムラインを用いて複数フレームにまたがる演出（エフェクト・サウンド・カメラ揺れなど）** を管理する。

- 戦闘ロジック自体（ダメージ計算）は UC-21 側
- 演出のタイミング管理を TSQ1 に任せる

---

### 1. スキル発動 → TSQ1 再生要求

プレイヤーがスキルを使用：

```rust
fn skill_use_system(world: &mut World, events: &mut Vec<OscEvent>) {
    // 条件を満たしたら
    tsq1_player.play("skills/fire_slash");
}
```

**関与 crate**

- `kitu-tsq1`（TSQ1 プレイヤー）
- `game-timeline`（ゲーム固有タイムライン定義）

---

### 2. TSQ1 プレイヤーの tick 処理

`kitu-tsq1` は KituRuntime の tick フェーズのどこか（例：戦闘処理後）で呼ばれる：

```rust
fn timeline_update_system(world: &mut World, events: &mut Vec<OscEvent>) {
    let mut player = world.resource_mut::<Tsq1Player>();
    player.update(world.time().delta(), events);
}
```

TSQ1 の各イベントは、

- `/render/effect/spawn`
- `/render/camera/shake`
- `/sound/se/play`

などの OSC-IR として `events` に出力される。

---

### 3. Unity 側：エフェクト・SE・カメラ制御

**関与レイヤー**

- Unity: `EffectManager`, `CameraEffects`, `SoundManager`

```csharp
void OnEffectSpawn(OscEvent ev) {
    var key = ev.ReadString("prefab");
    var pos = ev.ReadVector3("position");
    SpawnEffect(key, pos);
}

void OnCameraShake(OscEvent ev) {
    _cameraShake.Play(ev.ReadFloat("amplitude"), ev.ReadFloat("duration"));
}
```

TSQ1 側が演出の順序とタイミングを決め、 Unity 側はそれを **忠実に再生するプレーヤー** として機能する。

---

## UC-60: HUD 更新（詳細フロー）

### この UC で検証したいアーキテクチャ上のポイント

- 複数の ECS コンポーネント（HP, MP, XP, Level など）を **HUD 用の 1 つのビュー・モデル** に集約できるか。
- HUD 更新トリガーを dirty フラグベースにし、**無駄なイベント送信を抑えつつ一貫した UI 更新** が行えるか。
- HUD の見た目変更を Unity 側だけで完結させ、バックエンドとのインターフェースを安定させられるか。

### 全体概要

プレイヤー HP/MP/ステータス、経験値、クエスト状態など、HUD に表示する情報を **Rust 側で集約して一つの UI イベントとして送信し、Unity 側はそれを描画するだけにする。**

---

### 1. HUD 状態集約システム

**関与 crate**

- `game-ecs-features`（`Stats`, `Level`, `Xp` など）
- `game-logic`

```rust
fn hud_state_gather_system(world: &World, events: &mut Vec<OscEvent>) {
    let player = find_player_entity(world);
    let stats = world.get::<Stats>(player).unwrap();
    let level = world.get::<Level>(player).unwrap();
    let xp = world.get::<Xp>(player).unwrap();

    // 何か変化があった場合のみ送る（dirty フラグなど）
    if !hud_dirty(world) { return; }

    events.push(OscEvent::ui_hud_update_full(stats, level, xp));
}
```

---

### 2. Unity 側：HUD View の更新

```csharp
void OnHudUpdate(OscEvent ev) {
    var hp = ev.ReadInt("hp");
    var maxHp = ev.ReadInt("max_hp");
    var level = ev.ReadInt("level");
    var xp = ev.ReadInt("xp");
    var xpToNext = ev.ReadInt("xp_to_next");

    _hpBar.Set(hp, maxHp);
    _levelText.text = $"Lv {level}";
    _xpBar.Set(xp, xpToNext);
}
```

HUD の見た目を変更したい場合は Unity 側のみを修正すればよく、 ゲームロジックに影響を与えない。

---

## UC-61: ポーズ / メニュー（詳細フロー）

### この UC で検証したいアーキテクチャ上のポイント

- ゲームシミュレーション状態（Running / Paused）を `GameState` として管理し、**tick 実行と UI 表示を同期** できるか。
- ポーズメニュー表示・非表示を `/ui/menu/*` イベントに集約し、Unity の Time.timeScale 制御と両立させられるか。
- 一部システムのみ動かす「ソフトポーズ」などへの拡張余地があるか。

### 全体概要

`/input/pause` を契機に、「ゲームシミュレーションの停止 / 再開」と「ポーズメニュー UI の表示」を同期させる。

---

### 1. Unity → `/input/pause` 送信

```csharp
if (Input.GetKeyDown(KeyCode.Escape)) {
    KituNative.SendInputPauseToggle();
}
```

---

### 2. Rust 側：Pause 状態のトグル

**関与 crate**

- `kitu-runtime`（グローバルステート）

```rust
pub enum GameState { Running, Paused }

fn pause_toggle_system(world: &mut World, events: &mut Vec<OscEvent>) {
    let mut state = world.resource_mut::<GameState>();
    match *state {
        GameState::Running => {
            *state = GameState::Paused;
            events.push(OscEvent::ui_menu_show());
        }
        GameState::Paused => {
            *state = GameState::Running;
            events.push(OscEvent::ui_menu_hide());
        }
    }
}
```

`KituRuntime` の tick ループ側では、`GameState::Paused` の場合に

- 一部システムの実行をスキップ
- あるいは `step_one_tick` 自体を止める

などの挙動を選べるように設計しておく。

---

### 3. Unity 側：ポーズメニュー表示

```csharp
void OnMenuShow(OscEvent ev) {
    _pauseMenuRoot.SetActive(true);
    Time.timeScale = 0f; // Unity 側のエフェクト停止など
}

void OnMenuHide(OscEvent ev) {
    _pauseMenuRoot.SetActive(false);
    Time.timeScale = 1f;
}
```

- 物理エフェクトやパーティクルなど、Unity 内部時間に依存する表現を止めるかどうかはここで制御

---

## UC-70: TMD ホットリロード（詳細フロー）

### この UC で検証したいアーキテクチャ上のポイント

- TMD → Datastore → ECS という **データパイプラインが再入可能** であり、ランタイム中の差し替えに耐えられるか。
- どの程度まで自動で ECS に反映するか（完全再構築 vs 一部更新）のポリシーを、アーキテクチャとして整理できるか。
- ホットリロード時の失敗（パースエラー等）を `/debug/*` イベントで安全に扱えるか。

### 全体概要

開発中に TMD ファイルを編集した際、ゲームを再起動せずにデータのみ差し替える仕組み。

- ファイル監視は Rust または外部ツール（CLI）で実行
- 変更があった TMD を再ロードし、`datastore` に差し替え
- 影響範囲の ECS へ反映

---

### 1. ファイル監視

**関与 crate**

- （オプション）`notify` などのファイル監視ライブラリを利用

```rust
fn watch_tmd_changes(config: &StellaConfig) {
    // デバッグビルドのみ有効
}
```

変更検知時：

- 対象パスから TMD を再読み込み
- `game-data-build` の同じパイプラインを通す

---

### 2. データ差し替えと ECS への反映

```rust
fn reload_tmd(path: &Path, world: &mut World, events: &mut Vec<OscEvent>) {
    let mut datastore = world.resource_mut::<DataStore>();
    datastore.reload_from_tmd(path)?;

    // 影響するエンティティのステータスを再構築するなど
    recompute_from_datastore(world, &datastore);

    events.push(OscEvent::debug_tmd_reloaded(path));
}
```

- どこまで自動反映するかは設計次第（安全のため最小限としてもよい）

---

## UC-72: Rhai スクリプト変更 → ホットリロード（詳細フロー）

### この UC で検証したいアーキテクチャ上のポイント

- Rhai エンジンのコンパイル / インストールをランタイム中に差し替え、**ゲームを落とさずにスクリプト更新** できるか。
- スクリプトエラーを `/debug/script/*` イベントで通知し、UI 側（Web Admin 等）からフィードバックできるか。
- スクリプトがステートフルなオブジェクトに触れる場合でも、再ロード戦略を整理できるか。

### 全体概要

Rhai スクリプトを編集 → 保存したときに、ゲームを落とさず **スクリプトだけ差し替えて挙動を即時確認**できるようにする。

---

### 1. スクリプトファイルの監視

**関与 crate**

- `game-scripts`

```rust
fn watch_script_changes() {
    // デバッグ専用。スクリプトディレクトリを監視。
}
```

---

### 2. スクリプトの再コンパイルと差し替え

```rust
fn reload_script(path: &Path, world: &mut World, events: &mut Vec<OscEvent>) {
    let source = std::fs::read_to_string(path)?;
    let mut engine = world.resource_mut::<RhaiEngine>();

    match engine.compile_and_install(path, &source) {
        Ok(()) => events.push(OscEvent::debug_script_reloaded(path)),
        Err(err) => events.push(OscEvent::debug_script_error(path, err.to_string())),
    }
}
```

- エラー時はゲームを落とさず `/debug/script/error` イベントとして Web Admin などに通知

---

## UC-80: Kitu Shell からデバッグコマンド（詳細フロー）

### この UC で検証したいアーキテクチャ上のポイント

- CLI ベースの Shell から送るコマンドを、**すべて `/debug/*` イベントにマッピング** できるか。
- Shell と Web Admin が同じコマンド体系 / ルーティングを共有し、デバッグ機能を一箇所にまとめられるか。
- 実運用時にも安全なデバッグ権限・トランスポート層設計に拡張できるか。

### 全体概要

CLI ベースの Kitu Shell から、Runtime に対して `/debug/*` イベントを送り、 敵スポーンやフラグ操作などを行う。

---

### 1. Shell → Runtime への接続

**関与 crate**

- `kitu-shell`
- `kitu-transport`（TCP / WebSocket 等）

```text
kitu-shell connect ws://localhost:9000
```

Shell は `kitu-transport` を通じて Runtime と接続し、OSC-IR メッセージを送信する。

---

### 2. デバッグコマンドの送信

例：

```text
spawn_enemy goblin_lv10
set_flag quest_sword_completed
warp_player 10 0 5
```

これらは内部的に：

```text
/debug/spawn_enemy { kind: "goblin_lv10" }
/debug/set_flag { id: "quest_sword_completed" }
/debug/warp_player { x: 10, y: 0, z: 5 }
```

として Runtime に送られる。

---

### 3. Runtime 側での処理

**関与 crate**

- `kitu-runtime`
- `game-shell-ext`（コマンド → 実処理）

```rust
fn handle_debug_event(ev: OscEvent, world: &mut World, events: &mut Vec<OscEvent>) {
    match ev.address.as_str() {
        "/debug/spawn_enemy" => { /* UC-20 と同じ */ }
        "/debug/set_flag" => { /* UC-41 と同じ */ }
        "/debug/warp_player" => { /* Position を直接変更 */ }
        _ => {}
    }
}
```

結果やログは `/debug/log` として Shell・Web Admin に返す。

---

## UC-81: Web Admin で状態監視（詳細フロー）

### この UC で検証したいアーキテクチャ上のポイント

- Runtime の内部状態を **スナップショット JSON** としてエクスポートし、外部ツールから安全に閲覧できるか。
- ログやデバッグイベントのストリームを、WebSocket 経由でフロントに届ける **監視用チャネル** として整理できるか。
- 監視専用の API 群が、ゲーム本体のシミュレーションパスと過度に絡まないようにできているか。

### 全体概要

ブラウザ上の Web Admin から、Runtime の内部状態（ECS スナップショット・ログなど）をリアルタイムで監視する。

---

### 1. Web Admin Backend と Runtime の接続

**関与 crate**

- `kitu-web-admin-backend`
- `kitu-transport`

Backend は Runtime と WebSocket で接続し、

- `/debug/log`
- `/debug/state/*`

などのストリームを購読する。

---

### 2. ECS 状態スナップショットの取得

例：プレイヤーと敵の位置一覧を取得する API：

```rust
fn build_state_snapshot(world: &World) -> StateSnapshot {
    // ECS から必要な情報をかき集めて JSON にまとめる
}
```

Web Admin からの HTTP リクエスト：

```http
GET /api/state
```

に対し、`StateSnapshot` を JSON で返す。フロントエンドではこれをテーブルやミニマップなどで可視化する。

---

### 3. ログ・デバッグイベントのストリーム表示

- Runtime → Backend へ `/debug/log` をストリーミング
- Backend → ブラウザへ WebSocket 経由で転送

ブラウザ側はログビューアとして表示するだけで、 **実際のゲーム状態変更は行わない**。

---

## UC-82: リプレイ（入力再生）（詳細フロー）

### この UC で検証したいアーキテクチャ上のポイント

- `/input/*` イベント + tick 番号だけを記録すれば、**決定的に同じ結果を再現できる** ランタイムになっているか。
- オフライン Runner（Unity なし）とオンライン再生（Unity あり）で、同じバックエンドを共有できるか。
- 将来的なネットワーク同期（rollback 等）にも転用可能なログフォーマットになっているか。

### 全体概要

プレイヤー入力をログとして保存し、後から **まったく同じ結果を再生できるようにする** 機能。

- 入力ログは `/input/*` イベント + tick 番号の列
- 再生時には、録画と同じ順序・同じ tick でイベントを Runtime に流す

---

### 1. 録画：入力イベントの記録

**関与 crate**

- `kitu-runtime`
- `kitu-replay-runner`

```rust
struct InputRecord {
    tick: u64,
    event: OscEvent,
}
```

`KituRuntime` の tick 処理中に、受け取った `/input/*` を

- 現在の tick 番号付きで記録
- ファイル（JSON / MessagePack など）に保存

---

### 2. 再生：オフライン Runner

`kitu-replay-runner` は、録画済みのログファイルを読み込み、 スタンドアロンの `KituRuntime` インスタンスに対して tick を進めながら入力イベントを注入する。

```rust
fn replay(log: Vec<InputRecord>, runtime: &mut KituRuntime) {
    let mut current_tick = 0;
    for rec in log {
        while current_tick < rec.tick {
            runtime.step_one_tick();
            current_tick += 1;
        }
        runtime.enqueue_input(rec.event.clone());
    }
}
```

Unity を起動せずに **バックエンドだけで結果を検証** することも、 Unity をつないで **映像付きで再生** することも可能。

---

## UC-83: Web Admin から Kitu Shell コマンド実行（詳細フロー）

### この UC で検証したいアーキテクチャ上のポイント

- Web Admin からのコマンド入力を、Shell と **同じ構文・同じ処理系** に通せるか。
- 結果やログを WebSocket で返すことで、ブラウザ上から **双方向にデバッグ操作** できるか。
- コマンド実行パスが `/debug/*` に閉じており、本番環境での制御も設計しやすいか。

### 全体概要

Web Admin の UI から Shell と同等のデバッグコマンドを呼び出し、 Runtime に `/debug/*` イベントを送る。

---

### 1. Web Admin UI：コマンド入力フォーム

例：

- `spawn_enemy goblin_lv10`
- `set_flag quest_sword_completed`

ブラウザ → Backend:

```http
POST /api/debug/command
{ "command": "spawn_enemy goblin_lv10" }
```

---

### 2. Backend → Runtime：Shell と同じパスで送信

**関与 crate**

- `kitu-web-admin-backend`
- `kitu-shell`（パーサ再利用も可）

Backend はコマンド文字列をパースし、 内部的には Shell と同じように `/debug/*` イベントとして Runtime に送信する。

---

### 3. 結果表示

Runtime から返ってきた `/debug/log` を WebSocket 経由で UI に表示し、 「コマンドの結果」が即座に見えるようにする。

---

## UC-90: セーブ・ロード（詳細フロー）

### この UC で検証したいアーキテクチャ上のポイント

- ゲーム状態を `SaveData` として **シリアライズ可能な境界面** に切り出せているか（ECS / リソース → Save）。
- ロード時にワールドを再構築し、UC-01 の起動フローと整合する形で **シーンを復元** できるか。
- 保存形式（JSON / MessagePack / SQLite）を差し替えても、Runtime 側のインターフェースを安定させられるか。

### 全体概要

ゲーム進行状況（プレイヤー位置・ステータス・クエスト・シナリオフラグなど）を ファイルまたは SQLite に保存し、後から復元できるようにする。

---

### 1. セーブ要求（Unity → Rust）

Unity 側メニューなどから：

```csharp
public void OnSaveRequested() {
    KituNative.SendInputSave(slot: 1);
}
```

Rust 側には：

```text
/address: "/input/save"
args: { slot: 1 }
```

が送られる。

---

### 2. セーブデータの構築

**関与 crate**

- `game-logic`
- `game-data-schema`

```rust
struct SaveData {
    player: PlayerSave,
    quests: QuestLogSave,
    flags: ScenarioFlagsSave,
    time: GameTime,
}

fn build_save_data(world: &World) -> SaveData {
    // ECS / リソースから必要な情報を抽出
}
```

`SaveData` を JSON / MessagePack / SQLite など、 プロジェクト方針に応じたフォーマットで永続化する。

---

### 3. ロード処理

`/input/load { slot: 1 }` を受け取ったら：

1. セーブファイルを読み込む
2. 現在の ECS ワールドをクリア or リセット
3. `SaveData` から ECS / リソースを再構築
4. `/render/*` `/ui/*` の初期イベントを再出力

これにより、**起動直後と同様のフロー（UC-01）をセーブ状態で再現**できる。

---
