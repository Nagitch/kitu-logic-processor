# Unity-only アリーナゲーム 検証記録

確認日: 2026-09-05

対象: [企画・仕様書](unity-only-arena-game.md)を実装した `Assets/KituDemoApp/EndlessArena/EndlessArena.unity`。
初回検証ではUnity `6000.5.0f1` で **EditMode 39件、PlayMode 10件が成功**し、macOS版のビルド・起動・終了・設定保存を確認した。
テスト名、結果、実行日時、対象ソースのSHA-256、実シーンの進行記録は [results.json](../verification/unity-only-arena/results.json) に保存した。対象は同ファイルに記録した作業ツリーの差分であり、マージ済みのリビジョンを意味しない。

## PR作成時の再検証（Unity 6.6）

最新の `develop` を取り込み、現行プロジェクトのUnity `6000.6.0f1` でEditMode **39/39**、PlayMode **10/10** の成功を確認した。詳細は [Unity 6.6の結果](../verification/unity-only-arena/unity-66-results.json)、[ボス画面](../verification/unity-only-arena/unity-66/boss.png)、[リザルト](../verification/unity-only-arena/unity-66/result.png)を参照する。以下の6.5の記録は初回の比較証拠として保持する。

初回インポート中の撮影でUnityの非同期シェーダーコンパイル用の水色表示を取得したため、実シーンのテスト中だけ `EditorSettings.asyncShaderCompilation` を無効にし、終了時に元へ戻すようにした。ゲーム本体・通常Editor設定の変更ではない。[UnityのAPI説明](https://docs.unity3d.com/cn/6000.0/ScriptReference/EditorSettings-asyncShaderCompilation.html)

**6.6のmacOSビルドには環境上の制約が残る。** Burstの `llvm-lipo` はインストール先で実行権限がない状態（0644）だったため、universal binaryの生成が `Access denied` で停止した。所有者への実行権限付与もOSに拒否され、インストール済みツールは変更していない。6.6でのスタンドアロンビルド成功は主張しない。6.5でのビルド・ネイティブ操作成功とは分けて扱う。

## 証拠と確認範囲

| ID | 証拠 | 実施結果・範囲 |
| --- | --- | --- |
| M1 | `ArenaInventoryTests` | 15件成功。所持品の個体・所在、交換失敗時の原状維持、消費・強化、シールド吸収・時計・端数 |
| M2 | `ArenaSimulationTests` | 23件成功。戦闘、AI、命中寿命、同時死亡、フロア進行・難易度・持ち越し・結果固定。`EndlessProgressionThroughTwentyOneFloorsUsesSpecifiedRosterAndRewards` は21Fまで確認 |
| M3 | `ArenaRunScenarioTests.StockChestLoadoutAndContinuousMovementCanClearElevenFloors` | 1件成功。通常の箱・装備・移動・エイム・攻撃で11Fをクリア。HP・敵位置・武器性能を書き換えない |
| P1 | `ArenaFrontendTests` | 9件成功。仮想Keyboard/MouseをInput Systemへ入力し、本体の `ArenaGame.Update` を通してWASD、マウスエイム、両武器、Z/X、E、Tab、Esc、UI停止・設定を確認 |
| P2 | `ArenaSceneScenarioTests.ActualSceneStockRunReachesElevenThenDiesAndRestarts` | 1件成功。保存済みの実シーンを読み込み、0F・5F・10Fの箱を利用して11Fへ。敵による自然死、結果固定、Input SystemのR入力による再挑戦、Editor内Quitを確認 |
| U1 | 実GameViewのPNGと目視確認 | [オープニング](../verification/unity-only-arena/opening.png)、[箱と装備](../verification/unity-only-arena/chest.png)、[通常戦闘](../verification/unity-only-arena/combat.png)、[ボス予告](../verification/unity-only-arena/boss.png)、[リザルト](../verification/unity-only-arena/result.png)。Unityが描画した画像であり、別の描画実装によるモックではない |
| S1 | ネイティブアプリのUI操作 | Start→0F、Tab所持品、Escで閉じる・ポーズ、メニュー復帰を確認。最終ビルドで全画面切替、設定保存、Quit、再起動、キャンセル、初期値適用を確認 |
| B1 | `ArenaBuild.BuildMac` | 終了コード0。`Build Finished, Result: Success.` と `Arena player built` を確認。生成物は `Builds/EndlessArena.app`、対象シーンは新アリーナのみ |
| C1 | 現行ソース・シーン・asmdef・Build Settings | 入力・UIは `ArenaGame`、規則は `ArenaSimulation`、所持品・HPは `ArenaInventory`、表示は `ArenaHud` / `ArenaWorldView`。Unity-only経路にKitu/Rust/ネットワーク起動依存なし |

テストソースは Unityプロジェクトの `Assets/KituDemoApp/EndlessArena/Tests/Editor/` と `Tests/PlayMode/` にある。完全なテスト名と合否はresults.jsonから追跡できる。

### 実行結果

- EditMode: 39 passed / 0 failed、終了コード0、終了日時 `2026-09-05 12:58:23Z`。
- PlayMode: 10 passed / 0 failed、終了コード0、終了日時 `2026-09-05 13:07:53Z`。Rキー・Editor内Quitの追加確認を含む最終結果。
- P2は0F・5F・10Fの箱をすべて利用したことを検査し、11F入場時に最大HP130、攻撃倍率1.15、クリア10、ボス撃破2を確認した。攻撃・移動を止めると通常の敵AIでHP0になり、RでHP100・初期武器・他の枠空・強化なしへ戻った。
- S1では音量36%・ボーダーレス全画面を適用。Quit後にPID `78599` が存在しないことを確認し、再起動したPID `78908` の設定画面で36%・全画面が復元された。初期値→キャンセルでは36%が維持され、初期値→適用でウィンドウへ戻った。
- U1で見つかった見出しの切れ、カメラ描画領域外の文字残像、パネル越しのラベル残りは修正し、最終画像で確認した。
- 新アセットのメタファイルとGUIDの一意性、ソースの空白差分、ローカル文書リンクを確認した。元の `KituDemoAppMain.unity` と `UnityOnlyActionRpg/Runtime` に差分はない。

### 方法の区別

P1/P2のキー入力は仮想デバイスから本体のInput System処理へ送る。すべての操作を人が物理キーボードで手動再現したという記録ではない。
P2の連続攻略は `ArenaGame.AdvanceSimulation` に通常入力を渡してゲーム時間を加速する。`Update`、表示の `LateUpdate`、HUDの `OnGUI` は有効なまま、敵・プレイヤーのHPや位置、装備性能、フロア番号を直接改変しない。
M1/M2/P1の境界テストでは、同時死亡や満杯交換などを狙うために制御された初期状態を設定する。21F構成テストと、通常装備で攻略するM3/P2は異なる証拠である。
音源は初版に含めていないため、音量の適用・保存とAudioListenerへの反映まで確認し、実音のミュート確認は対象外。フォーカス喪失はP1で実際のコールバック処理を呼んで停止・復帰時の停止維持を確認している。

## 仕様第14章の受け入れ条件との対応

A01〜A26は仕様書のチェックボックスの順番。以下は自動実行、実画面、ネイティブ操作、コード確認を組み合わせた判定であり、各行を個別の手動プレイで確認したという意味ではない。

| ID | 要件 | 実施した証拠・結果 |
| --- | --- | --- |
| A01 | Unityのみでメニューから0Fへ | B1・S1・P2。単一コントローラ、準備時の敵0、箱とポータル、開始前の停止を確認 |
| A02 | 設定の入口と戻り先 | P1 `SettingsDraftCancelResetAndApplyReturnToOpeningWithoutAdvancingRun` / `PauseSettingsAndFocusLossFreezeCombatAndReturnToPause`。両入口、適用・取消、ポーズ維持 |
| A03 | 設定の編集・適用・保存 | P1の設定2件と `SettingsSaveLoadUsesAnIsolatedPrefixAndRejectsInvalidSavedValues`、S1。実音は上記の範囲外 |
| A04 | 画面モード・終了・Editor保護 | B1・S1。P2はEditor内Quit後もOpeningを保持して最後まで実行成功 |
| A05 | メニュー復帰後の新規ラン | P1 `OpeningStartAndReturnToMenuOwnTheSceneAndRunLifetime`、M2 `DeathSnapshotStaysFixedAndRetryResetsRunWhileMenuClearsIt`、S1。設定はラン状態から独立 |
| A06 | WASD・マウス・両武器 | P1 `WasdAimAndBothMouseButtonsReachTheRunningSimulation`、M2 `IndependentWeaponsHitConeOncePerEnemyAndHonorCooldowns` / `MovementIsNormalizedWallBoundAndMaintainsLastValidAim` |
| A07 | 雑魚3種とボス | M2の敵AI・ボス周期テスト、P2の全敵構成での連続攻略と自然死、U1 |
| A08 | 全滅後にポータル1個 | M2の進行・重複移動・同時死亡テスト、P2の全クリアと表示オブジェクト数確認 |
| A09 | 5F報酬を経て6Fへ | P2の入場ログと0/5/10F箱利用の集合検査。取得・装備・強化は本体のUIコマンドを使用 |
| A10 | 10F後も11Fへ | M2の21F構成、M3の11Fクリア、P2の11F入場・敵AI。勝利終了なし |
| A11 | 難易度上昇・敵数上限 | M2の21F構成テスト。各フロアの敵種・個体数・HP・ダメージを照合 |
| A12 | 敵アイテムドロップなし | C1の撃破処理と生成物、M2/P2の通常クリア時の箱なし・ボス報酬のみ確認 |
| A13 | 持ち切れない箱・満杯交換 | M1 `ChestHasSixDistinctWeaponCandidatesAndFiveItemUpgradeCandidates` / `TakingSwappingAndEquippingConserveItemInstancesEvenWhenBackpackIsFull`、U1 |
| A14 | 箱・所持品の個体保存 | M1の交換・型・個体保存、M2 `BossClearHealsOnceAndKeepsRewardContentsAndShieldCharge`。UI開閉はC1/P1、表示はU1 |
| A15 | 強化の一回消費 | M1 `UpgradesConsumeOnceAndHealthUpgradeDoesNotRefillAnExistingShield`、P2の3回の箱強化、U1の強化値 |
| A16 | Z/X、救急キット・手榴弾 | P1 `ZAndXUseTheirOwnSlotsThroughTheInputSystem`、M2 `GrenadeRequiresAimConsumesAtThrowAndDealsSnapshotAreaDamageAfterDelay`、M1の消費・空き枠 |
| A17 | 不成立時・長押し・UI復帰 | M1/M2の満タンキット・空振り投擲、P1 `FullHealthMedkitAndSimultaneousMedkitsDoNotConsumeAnExtraItem` / `HeldButtonsAcrossInventoryCloseRequireReleaseBeforeWeaponsOrItems` |
| A18 | シールド吸収・容量・枯渇保持 | M1 `ShieldsAbsorbInSlotOrderAndRemainEquippedWhenEmpty` / `FullHealthMedkitIsKeptAndUsedMedkitOnlyRefillsHp`、P2自然死、U1 |
| A19 | 複数シールドとキー無作用 | M1のA→B→HP吸収・UseItem拒否、C1/P1の共通スロット入力経路 |
| A20 | シールド時計と停止 | M1の待ち時間・端数・未装備回復、P1のFrozenState、M2のフロア持ち越し |
| A21 | 残量の個体保持・HP回復分離 | M1 `ShieldChargeAndFractionSurviveSwapsButOnlyEquipmentRegenerates` / `HealingDoesNotTouchShieldOrDelayAndNonPositiveDamageDoesNothing`、M2持ち越し |
| A22 | HP・装備・強化の持ち越し | M2 `FloorTransitionCarriesHealthInventoryAndShieldTimersButResetsTransientState` / `BossClearHealsOnceAndKeepsRewardContentsAndShieldCharge`、P2 |
| A23 | 空枠・武器なし・重なり・UI | M2 `PortalRejectsNoWeaponWithoutDiscardingChest` / `PortalAppearingUnderPlayerRequiresExitAndReentryAndCannotTransitionTwice`、M1無効操作、P1入力遮断 |
| A24 | 同時死亡・残存攻撃 | M2 `SimultaneousPlayerAndLastEnemyDeathRecordsKillButNeverClearsOrRewards`（通常/ボス2ケース）/ `ClearRemovesHostileShotsAndGrenadesBeforeSafeStateCanBeDamaged` |
| A25 | 結果固定・再挑戦 | M2の死亡スナップショット、P2の自然死後の入力・時間による不変性とRによる完全初期化、U1。再挑戦ボタンは同じStartRun呼出しをC1で確認 |
| A26 | UI中の全時計停止 | P1の `FrozenState.AssertUnchanged` と設定・所持品テスト、M2の遷移停止。HP、敵、弾、手榴弾、CD、シールド、計時を比較 |

## 詳細仕様の補足確認

| ID | 対象 | 証拠・確認範囲 |
| --- | --- | --- |
| D01 | 状態遷移・一括生成 | M2の進行・ポータルテスト、P2全フロアログ・再挑戦 |
| D02 | 報酬任意・前フロア破棄 | M2 Opening/進行/持ち越し、P1初期武器の入場、C1の箱破棄処理 |
| D03 | アリーナ・カメラ・壁 | P1正投影確認、M2正規化・壁・非接触ダメージ、P2実シーン、U1 |
| D04 | 床エイム・無効方向 | P1 WASD/マウス、M2 `MovementIsNormalizedWallBoundAndMaintainsLastValidAim`、投擲無効入力 |
| D05 | 固定配置・初回待機 | M2生成・AIの初回攻撃待機、P2各入場の敵数、C1配置表 |
| D06 | E/Tab/Esc/Rの経路 | P1 `ChestDistanceAndCombatInventoryGatesUseEAndTab`、設定/ポーズテスト、P2のR再挑戦、S1のTab/Esc |
| D07 | フォーカス喪失 | P1で `OnApplicationFocus(false/true)` の本体処理を通し、復帰だけでは再開しないことを確認。OSアプリ切替を全戦闘場面で手動再現したものではない |
| D08 | 3武器の数値・独立CD | M1固定候補、M2近接・弾寿命・CD、P1両射撃武器、U1性能比較 |
| D09 | 命中寿命・非貫通 | M2 `ProjectileSweepHitsNearestTargetEvenWhenListOrderIsFarFirst` / `BulletSpawnOverlappingEnemyStillHitsInsteadOfSkippingCloseTarget` / `BulletsExpireAtWallOrMaximumRange` |
| D10 | ダメージ・撃破の一意性 | M1 HP下限/死亡後拒否、M2同時死亡・近接重複・味方弾無作用、C1丸め処理 |
| D11 | 雑魚AIの待機・射程 | M2 `ActualPursuerMovementAndMeleeEventuallyKillAnIdlePlayer` / `ContactAloneDoesNoDamageAndEnemyWaitsInitialAttackInterval` / `ShooterApproachesAndFiresTowardPlayerAfterItsInitialDelay`、P2混成敵、C1重装型共有処理 |
| D12 | ボス周期 | M2 `BossTelegraphsThenFiresEightShotsAndDoesNotMeleeDuringTellOrRecovery`、P2の5F/10F、U1予告 |
| D13 | 所在・型・破棄 | M1個体保存・無効操作・解除空き枠、P2実在庫操作、U1、C1破棄先なし |
| D14 | 消費順・復活禁止 | M1同時キット/死亡、M2 `MedkitInputsAreOrderedBeforeIncomingDamageAndSecondFullHealthMedkitIsKept`、P1同時キー |
| D15 | 手榴弾の位置・飛行 | M2 `GrenadeTargetIsLimitedByThrowRangeAndArena` / `GrenadeRequiresAimConsumesAtThrowAndDealsSnapshotAreaDamageAfterDelay`、P1実入力 |
| D16 | 手榴弾の範囲・スナップショット | M2投擲・範囲・クリア時消去、M1消費後の予備非補充 |
| D17 | 共通回復時計・被弾優先 | M1 `DamageUpdateCannotRegenerateAndRecoveryStartsAfterThreeSeconds` / `DamageWithoutAShieldStillStartsSharedDelayAndChangingEquipmentKeepsIt`、P1停止 |
| D18 | 回復端数・容量 | M1 `FractionalRecoveryAccumulatesAcrossSmallUpdatesAndUsesNewCapacity` / `ShieldChargeAndFractionSurviveSwapsButOnlyEquipmentRegenerates`。最大HP減少は初版の通常操作にはなく、容量への切詰め分岐はC1確認 |
| D19 | 強化値・持続・リセット | M1強化・回復分離、P2最大HP130/倍率1.15→Rで100/1.00、C1 XP/速度強化なし |
| D20 | 固定11品・ボス表再利用 | M1固定箱テスト、M2各ボス箱、P2の0/5/10F箱利用、U1 |
| D21 | 持ち越し全項目 | M2 `FloorTransitionCarriesHealthInventoryAndShieldTimersButResetsTransientState` と死亡スナップショット、P1ラン状態破棄、S1設定保存 |
| D22 | メニュー内容・再入防止 | P1ラン寿命、P2 Editor Quit/R、U1/S1メニュー、C1のStartRun状態ガードと中断時の破棄 |
| D23 | 設定初期値・不正値 | P1設定3テスト、S1取消/復元/初期値適用。Editor画面変更はコンパイル分岐で除外 |
| D24 | HUD・図形 | U1の各画面とS1所持品画面、P2表示オブジェクト数、C1表示項目。色・図形・数値のプレースホルダ |
| D25 | 所持品UI | U1箱・装備・能力値、P2本体の取得/装備/破棄/強化コマンド、M1満杯交換・型拒否、C1各ボタン接続 |
| D26 | リザルト | M2スナップショット、P2の11F・クリア10・撃破58・ボス2・HP0、U1リザルトと照合 |
| D27 | 最小コンテンツ・Unity-only | C1で図形生成と依存範囲を確認、P1/P2で実行、U1/S1で表示、B1配布プレイヤー |

## 再実行

Unityプロジェクト `kitu-integration-runner/unity-demo-game/kitu-unity-demo-game/` を作業ディレクトリにし、そのパスを開いたEditorを終了して順に実行する。通常チェックアウトを開いている別Editorとは作業パスを混同しない。

```sh
arena_editor='/Applications/Unity/Hub/Editor/6000.6.0f1/Unity.app/Contents/MacOS/Unity'
mkdir -p Logs
"$arena_editor" -batchmode -nographics -projectPath "$PWD" \
  -runTests -testPlatform EditMode -testFilter UnityOnlyArena.Tests \
  -testResults Logs/arena-editmode.xml -logFile Logs/arena-editmode.log
"$arena_editor" -projectPath "$PWD" \
  -runTests -testPlatform PlayMode -testFilter UnityOnlyArena.Tests \
  -testResults Logs/arena-playmode.xml -logFile Logs/arena-playmode.log
"$arena_editor" -batchmode -quit -projectPath "$PWD" \
  -executeMethod UnityOnlyArena.Editor.ArenaBuild.BuildMac \
  -logFile Logs/arena-build.log
```

PlayModeを非バッチで実行すると、実シーンの5画像を `Logs/arena-visuals/` に出力する。`-batchmode` を追加した場合はスクリーンショットを明示的にスキップし、テスト本体だけを実行する。テストには `-quit` を併記せず、結果XMLの実行件数と失敗数を確認する。

生成物・ログはGit管理対象外である。共有する結果は本書とresults.json、画像5枚として保存した。Unityが自動更新した既存の描画設定・サービス設定はこの変更の対象にせず元へ戻した。新しいシーンのBuild Settings追加は保持する。

## ペインの記録

[ペインログ](../unity-only-arena-pain-log.md)に、死亡・報酬・回復の順序、アイテムとシールドの状態寿命、Editor入力テストのタイミングを記録した。
最初の入力テストは2成功/7失敗だったが、本体Updateを迂回せずテスト用の入力タイミング・フォーカス設定を直して解消した。
実シーン検証では、箱への自動移動がポータルを横切る検証用経路を修正し、0/5/10Fの箱利用を追加検査した。意図的なチェックポイントログを未処理ログと誤判定していたテスト側の検査も修正した。最終結果を得るためにHP・攻撃力・フロア番号を書き換える対応は行っていない。
Kituによる改善は今後比較する仮説であり、この実装で実測した効果とは扱わない。

## Oblique camera follow-up (2026-09-06 JST)

The camera now looks down at the arena center at 55 degrees to the floor.
Unity 6000.6.0f1 passed all 10 existing PlayMode tests, including mouse aim,
with this camera. A separate interactive run of the saved-scene scenario
also passed and produced a [combat capture](../verification/unity-only-arena/oblique-camera.png).
The capture was inspected for the angled view and full arena framing.
Earlier captures linked above retain the original overhead camera as historical evidence.

## Player-follow camera follow-up (2026-09-06 JST)

Following the supplied reference image, the current camera uses perspective
projection with a 56-degree pitch and -72-degree yaw. It tracks the player's
body center at a fixed distance of 55 with a 13.1-degree vertical field of view.
Arena edges may leave the viewport. Movement input is mapped onto the camera's
ground-plane axes, and world labels are clipped to the gameplay viewport.
This supersedes the fixed orthographic framing documented in the earlier captures.

Unity 6000.6.0f1 passed **12/12 PlayMode tests** in an interactive Editor run
(2026-09-05 16:29:58–16:29:59 UTC). The tests cover all four movement directions,
mouse aiming and both weapons, unchanged camera offset/zoom at all four arena
corners, pause and restart, and player centering through the existing 11-floor
scene scenario. Centering allows one pixel for viewport rounding.
The [boss capture](../verification/unity-only-arena/follow-camera-boss.png) was
inspected for the diagonal view, larger actors, player centering and viewport
label clipping. Local runner details are in `Logs/arena-follow-playmode.xml`
and `Logs/arena-follow-playmode.log` (not committed).
