// Inspection v1 is an observer-only display contract. Wide identities stay exact
// decimal strings; original game projections and replay endpoints are unchanged.
export type U64 = string;
export type I64 = string;
export type Vec2 = { x: number; y: number };
export type Item = {
  id: number;
  kind: number;
  damage: number;
  shield: number;
  name: string;
  interval: number;
  shieldFraction: number;
};
export type Inventory = {
  health: number;
  maxHealth: number;
  nextItemId: number;
  attackUpgrades: number;
  attackMultiplier: number;
  shieldDelayRemaining: number;
  damagedThisUpdate: boolean;
  backpack: Item[];
  equipment: Item[];
  chest: Item[];
};
export type Enemy = {
  Id: number;
  Kind: number;
  Position: Vec2;
  Health: number;
  MaxHealth: number;
  Radius: number;
  Speed: number;
  Damage: number;
  AttackInterval: number;
  AttackRange: number;
  AttackCooldown: number;
  BossState: number;
  PhaseRemaining: number;
};
export type Projectile = {
  Id: number;
  Position: Vec2;
  Direction: Vec2;
  Speed: number;
  DistanceRemaining: number;
  Damage: number;
  EnemyOwned: boolean;
  Radius: number;
};
export type Grenade = {
  Id: number;
  Start: Vec2;
  Target: Vec2;
  Position: Vec2;
  Remaining: number;
  Damage: number;
};
export type Effect = {
  Id: number;
  Kind: number;
  Position: Vec2;
  Direction: Vec2;
  Radius: number;
  Remaining: number;
};
export type InspectionArenaState = {
  tick: I64;
  simulationSteps: U64;
  phase: number;
  overlay: string;
  floor: number;
  elapsed: number;
  playerPosition: Vec2;
  aimDirection: Vec2;
  inventory: Inventory;
  chestAvailable: boolean;
  portalAvailable: boolean;
  enemiesDefeated: number;
  bossesDefeated: number;
  floorsCleared: number;
  nextEntityId: number;
  transitionRemaining: number;
  portalArmed: boolean;
  weaponCooldowns: number[];
  enemies: Enemy[];
  projectiles: Projectile[];
  grenades: Grenade[];
  effects: Effect[];
  result: {
    present: boolean;
    floor: number;
    enemiesDefeated: number;
    bossesDefeated: number;
    floorsCleared: number;
    maxHealth: number;
    elapsed: number;
    attackMultiplier: number;
    equipmentNames: string[];
  };
};
export type Cue = {
  id: string;
  clipId: string;
  startedTick: I64;
  offsetTick: U64;
  nextEventIndex: number;
  eventCount: number;
};
export type BossCue = Cue & {
  entityId: number;
  floor: number;
  radius: number;
  intensity: number;
};
export type FloorCue = Cue & {
  fromFloor: number;
  toFloor: number;
  opacity: number;
};
export type InspectionPresentation = {
  contractVersion: 1;
  run: U64;
  tick: I64;
  simulationStep: U64;
  bosses: BossCue[];
  floor: FloorCue | null;
};
export type InspectionReplayMode = {
  active: boolean;
  recordingId: string | null;
  tick: I64;
  totalTicks: U64;
  playing: boolean;
  seeking: boolean;
  error: string | null;
};
export type InspectionScriptFault = {
  tick: I64;
  enemyId: number;
  scriptHash: string;
  diagnostic: {
    kind: string;
    message: string;
    line: number | null;
    column: number | null;
  };
};
export type InspectionEvent = {
  sequence: U64;
  revision: U64;
  epoch: U64;
  run: U64;
  tick: I64;
  bundleIndex: number;
  messageIndex: number;
  address: string;
  detail:
    | { kind: "message"; messageJson: string }
    | {
        kind: "run";
        contentHash: string;
        scriptHash: string;
        timelineHash: string;
      }
    | { kind: "oversize"; encodedBytes: U64; sha256: string };
};
export type InspectionEventWindow = {
  capacity: 256;
  byteLimit: 2097152;
  payloadByteLimit: 65536;
  firstSequence: U64 | null;
  lastSequence: U64 | null;
  dropped: U64;
  omittedPayloads: U64;
  entries: InspectionEvent[];
};
export type InspectionTimingSample = {
  attempt: U64;
  revision: U64;
  tick: I64;
  outcome: "idle" | "advanced" | "replacement" | "fault";
  runtimeAdvanced: boolean;
  simulationAdvanced: boolean;
  durationUs: number;
  lockWaitUs: number;
  durationClamped: boolean;
  lockWaitClamped: boolean;
  overBudget: boolean;
};
export type InspectionTiming = {
  scope: "hostUpdate";
  capacity: 256;
  budgetUs: number;
  maxDurationUs: 86400000000;
  totalSamples: U64;
  overBudgetSamples: U64;
  last: InspectionTimingSample | null;
  samples: InspectionTimingSample[];
  meanUs: number | null;
  p95Us: number | null;
  maxUs: number | null;
};
export type ArenaGeometry = {
  minX: number;
  maxX: number;
  minY: number;
  maxY: number;
  playerRadius: number;
  chest: Vec2 & { interactionRadius: number };
  portal: Vec2 & { triggerRadius: number };
};
export type InspectionSnapshot = {
  schemaVersion: 1;
  sessionId: string;
  revision: U64;
  attempt: U64;
  epoch: U64;
  run: U64;
  mode: InspectionReplayMode;
  readOnly: boolean;
  execution: { package: string; sourceHash: string; target: string };
  versions: Record<
    "content" | "script" | "timeline",
    { activeHash: string | null; pendingHash: string }
  >;
  diagnostics: {
    hostError: string | null;
    recordingError: string | null;
    scriptFault: InspectionScriptFault | null;
  };
  state: InspectionArenaState;
  presentation: InspectionPresentation;
  arena: ArenaGeometry;
  events: InspectionEventWindow;
  timing: InspectionTiming;
};
export const INSPECTION_BYTES = 8_388_608;
const U64_MAX = 18_446_744_073_709_551_615n;
const I64_MAX = 9_223_372_036_854_775_807n;
const utf8 = new TextEncoder();
function requireValue(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`Invalid Arena inspection: ${message}`);
}
function shape(value: unknown, names: string): Record<string, unknown> {
  requireValue(
    value !== null && typeof value === "object" && !Array.isArray(value),
    "expected object",
  );
  const fields = names.split(" ");
  const object = value as Record<string, unknown>;
  requireValue(
    Object.keys(object).length === fields.length &&
      fields.every((name) => Object.hasOwn(object, name)),
    `fields ${names}`,
  );
  return object;
}
function text(value: unknown, maximum = 4096): asserts value is string {
  requireValue(
    typeof value === "string" &&
      utf8.encode(value).length <= maximum &&
      !value.includes("\0"),
    "bounded text",
  );
}
function hash(value: unknown) {
  requireValue(
    typeof value === "string" && /^[0-9a-f]{64}$/.test(value),
    "SHA-256",
  );
}
function bool(value: unknown) {
  requireValue(typeof value === "boolean", "boolean");
}
function integer(value: unknown, minimum = -2147483648, maximum = 2147483647) {
  requireValue(
    typeof value === "number" &&
      Number.isInteger(value) &&
      value >= minimum &&
      value <= maximum,
    "integer range",
  );
}
function scalar(
  value: unknown,
  minimum = -3.4028234663852886e38,
  maximum = 3.4028234663852886e38,
) {
  requireValue(
    typeof value === "number" &&
      Number.isFinite(value) &&
      value >= minimum &&
      value <= maximum,
    "finite scalar",
  );
}
function nullable(value: unknown, validate: (value: unknown) => void) {
  if (value !== null) validate(value);
}
function list(
  value: unknown,
  validate: (value: unknown) => void,
  maximum = 65536,
  exact?: number,
): unknown[] {
  requireValue(
    Array.isArray(value) &&
      value.length <= maximum &&
      (exact === undefined || value.length === exact),
    "array bounds",
  );
  value.forEach((entry) => validate(entry));
  return value;
}
function vector(value: unknown) {
  const x = shape(value, "x y");
  scalar(x.x);
  scalar(x.y);
}
export function unsigned(value: unknown): bigint {
  requireValue(
    typeof value === "string" &&
      /^(0|[1-9][0-9]*)$/.test(value) &&
      value.length <= 20,
    "canonical u64 string",
  );
  const parsed = BigInt(value);
  requireValue(parsed <= U64_MAX, "u64 range");
  return parsed;
}
export function completedTick(value: unknown): bigint {
  requireValue(
    typeof value === "string" &&
      /^(?:-1|0|[1-9][0-9]*)$/.test(value) &&
      value.length <= 19,
    "canonical completed tick",
  );
  const parsed = BigInt(value);
  requireValue(parsed <= I64_MAX, "i64 tick range");
  return parsed;
}
function item(value: unknown) {
  const x = shape(value, "id kind damage shield name interval shieldFraction");
  ["id", "kind", "damage", "shield"].forEach((k) => integer(x[k]));
  text(x.name);
  scalar(x.interval);
  scalar(x.shieldFraction);
}
function inventory(value: unknown) {
  const x = shape(
    value,
    "health maxHealth nextItemId attackUpgrades attackMultiplier shieldDelayRemaining damagedThisUpdate backpack equipment chest",
  );
  ["health", "maxHealth", "nextItemId", "attackUpgrades"].forEach((k) =>
    integer(x[k]),
  );
  scalar(x.attackMultiplier);
  scalar(x.shieldDelayRemaining);
  bool(x.damagedThisUpdate);
  list(x.backpack, item, 3, 3);
  list(x.equipment, item, 4, 4);
  list(x.chest, item);
}
function actor(
  value: unknown,
  kind: "enemy" | "projectile" | "grenade" | "effect",
) {
  const fields = {
    enemy:
      "Id Kind Position Health MaxHealth Radius Speed Damage AttackInterval AttackRange AttackCooldown BossState PhaseRemaining",
    projectile:
      "Id Position Direction Speed DistanceRemaining Damage EnemyOwned Radius",
    grenade: "Id Start Target Position Remaining Damage",
    effect: "Id Kind Position Direction Radius Remaining",
  }[kind];
  const x = shape(value, fields);
  for (const [key, field] of Object.entries(x)) {
    if (["Position", "Direction", "Start", "Target"].includes(key))
      vector(field);
    else if (
      ["Id", "Kind", "Health", "MaxHealth", "Damage", "BossState"].includes(key)
    )
      integer(field);
    else if (key === "EnemyOwned") bool(field);
    else scalar(field);
  }
}
function state(value: unknown) {
  const x = shape(
    value,
    "tick simulationSteps phase overlay floor elapsed playerPosition aimDirection inventory chestAvailable portalAvailable enemiesDefeated bossesDefeated floorsCleared nextEntityId transitionRemaining portalArmed weaponCooldowns enemies projectiles grenades effects result",
  );
  completedTick(x.tick);
  unsigned(x.simulationSteps);
  integer(x.phase, 0, 5);
  text(x.overlay, 64);
  [
    "floor",
    "enemiesDefeated",
    "bossesDefeated",
    "floorsCleared",
    "nextEntityId",
  ].forEach((k) => integer(x[k]));
  scalar(x.elapsed);
  scalar(x.transitionRemaining);
  vector(x.playerPosition);
  vector(x.aimDirection);
  inventory(x.inventory);
  ["chestAvailable", "portalAvailable", "portalArmed"].forEach((k) =>
    bool(x[k]),
  );
  list(x.weaponCooldowns, scalar, 2, 2);
  for (const [field, kind] of [
    ["enemies", "enemy"],
    ["projectiles", "projectile"],
    ["grenades", "grenade"],
    ["effects", "effect"],
  ] as const) {
    const entries = list(x[field], (v) => actor(v, kind));
    const ids = entries.map((v) => (v as { Id: number }).Id);
    requireValue(new Set(ids).size === ids.length, "duplicate typed entity ID");
  }
  const result = shape(
    x.result,
    "present floor enemiesDefeated bossesDefeated floorsCleared maxHealth elapsed attackMultiplier equipmentNames",
  );
  bool(result.present);
  [
    "floor",
    "enemiesDefeated",
    "bossesDefeated",
    "floorsCleared",
    "maxHealth",
  ].forEach((k) => integer(result[k]));
  scalar(result.elapsed);
  scalar(result.attackMultiplier);
  list(result.equipmentNames, (v) => text(v), 4);
}
function cue(value: unknown, boss: boolean) {
  const x = shape(
    value,
    `id clipId startedTick offsetTick nextEventIndex eventCount ${boss ? "entityId floor radius intensity" : "fromFloor toFloor opacity"}`,
  );
  text(x.id, 128);
  requireValue(
    x.clipId === (boss ? "boss-telegraph" : "floor-transition"),
    "cue clip",
  );
  completedTick(x.startedTick);
  unsigned(x.offsetTick);
  integer(x.nextEventIndex, 0, 4294967295);
  integer(x.eventCount, 0, 4294967295);
  requireValue(
    (x.nextEventIndex as number) <= (x.eventCount as number),
    "cue event position",
  );
  if (boss) {
    integer(x.entityId);
    integer(x.floor);
    scalar(x.radius, 0);
    scalar(x.intensity, 0);
  } else {
    integer(x.fromFloor);
    integer(x.toFloor);
    scalar(x.opacity, 0, 1);
  }
}
function sample(value: unknown) {
  const x = shape(
    value,
    "attempt revision tick outcome runtimeAdvanced simulationAdvanced durationUs lockWaitUs durationClamped lockWaitClamped overBudget",
  );
  unsigned(x.attempt);
  unsigned(x.revision);
  completedTick(x.tick);
  requireValue(
    ["idle", "advanced", "replacement", "fault"].includes(x.outcome as string),
    "timing outcome",
  );
  [
    "runtimeAdvanced",
    "simulationAdvanced",
    "durationClamped",
    "lockWaitClamped",
    "overBudget",
  ].forEach((k) => bool(x[k]));
  scalar(x.durationUs, 0, 86400000000);
  scalar(x.lockWaitUs, 0, 86400000000);
  requireValue(
    !x.simulationAdvanced || x.runtimeAdvanced,
    "simulation requires Runtime advancement",
  );
  if (x.outcome === "idle")
    requireValue(
      !x.runtimeAdvanced && !x.simulationAdvanced,
      "idle cannot advance simulation",
    );
}
function event(value: unknown) {
  const x = shape(
    value,
    "sequence revision epoch run tick bundleIndex messageIndex address detail",
  );
  ["sequence", "revision", "epoch", "run"].forEach((k) => unsigned(x[k]));
  completedTick(x.tick);
  integer(x.bundleIndex, 0, 4294967295);
  integer(x.messageIndex, 0, 4294967295);
  text(x.address, 4096);
  requireValue(
    (x.address as string).startsWith("/game/arena/") ||
      [
        "/ui/arena/command",
        "/ui/arena/use",
        "/ui/arena/timeline/event",
      ].includes(x.address as string),
    "event address",
  );
  requireValue(
    x.detail !== null && typeof x.detail === "object",
    "event detail",
  );
  const kind = (x.detail as Record<string, unknown>).kind;
  if (kind === "message") {
    const detail = shape(x.detail, "kind messageJson");
    text(detail.messageJson, 65536);
    const wire = shape(
      JSON.parse(detail.messageJson as string),
      "address args",
    );
    requireValue(wire.address === x.address, "event wire address");
    list(wire.args, (argument) => {
      const arg = shape(argument, "type value");
      if (arg.type === "str") text(arg.value, 65536);
      else if (arg.type === "bool") bool(arg.value);
      else if (arg.type === "float") scalar(arg.value);
      else if (arg.type === "int") integer(arg.value);
      // The original JSON remains a string. Never show a parsed int64 as exact.
      else {
        requireValue(
          arg.type === "int64" &&
            typeof arg.value === "number" &&
            Number.isInteger(arg.value),
          "OSC scalar",
        );
      }
    });
  } else if (kind === "run") {
    const d = shape(x.detail, "kind contentHash scriptHash timelineHash");
    hash(d.contentHash);
    hash(d.scriptHash);
    hash(d.timelineHash);
    requireValue(x.address === "/game/arena/run", "run summary address");
  } else {
    const d = shape(x.detail, "kind encodedBytes sha256");
    requireValue(kind === "oversize", "event detail kind");
    requireValue(unsigned(d.encodedBytes) > 65536n, "oversize threshold");
    hash(d.sha256);
  }
}
function freeze<T>(value: T): T {
  if (value !== null && typeof value === "object") {
    Object.values(value).forEach(freeze);
    Object.freeze(value);
  }
  return value;
}
export function validateInspection(value: unknown): InspectionSnapshot {
  const x = shape(
    value,
    "schemaVersion sessionId revision attempt epoch run mode readOnly execution versions diagnostics state presentation arena events timing",
  );
  requireValue(x.schemaVersion === 1, "schema version");
  text(x.sessionId, 128);
  requireValue((x.sessionId as string).length > 0, "session ID");
  ["revision", "attempt", "epoch", "run"].forEach((k) => unsigned(x[k]));
  bool(x.readOnly);
  const execution = shape(x.execution, "package sourceHash target");
  text(execution.package);
  text(execution.target);
  hash(execution.sourceHash);
  const versions = shape(x.versions, "content script timeline");
  for (const version of Object.values(versions)) {
    const v = shape(version, "activeHash pendingHash");
    nullable(v.activeHash, hash);
    hash(v.pendingHash);
  }
  const mode = shape(
    x.mode,
    "active recordingId tick totalTicks playing seeking error",
  );
  ["active", "playing", "seeking"].forEach((k) => bool(mode[k]));
  completedTick(mode.tick);
  unsigned(mode.totalTicks);
  nullable(mode.error, (v) => text(v));
  nullable(mode.recordingId, hash);
  requireValue(x.readOnly === (mode.active || mode.seeking), "readonly mode");
  if (mode.active)
    requireValue(
      mode.recordingId !== null &&
        completedTick(mode.tick) < unsigned(mode.totalTicks),
      "replay range",
    );
  else
    requireValue(
      mode.recordingId === null &&
        mode.totalTicks === "0" &&
        mode.playing === false,
      "live mode",
    );
  const diagnostics = shape(
    x.diagnostics,
    "hostError recordingError scriptFault",
  );
  nullable(diagnostics.hostError, (v) => text(v));
  nullable(diagnostics.recordingError, (v) => text(v));
  nullable(diagnostics.scriptFault, (value) => {
    const f = shape(value, "tick enemyId scriptHash diagnostic");
    completedTick(f.tick);
    integer(f.enemyId);
    hash(f.scriptHash);
    const d = shape(f.diagnostic, "kind message line column");
    text(d.kind);
    text(d.message);
    nullable(d.line, (v) => integer(v, 0, Number.MAX_SAFE_INTEGER));
    nullable(d.column, (v) => integer(v, 0, Number.MAX_SAFE_INTEGER));
  });
  state(x.state);
  const s = x.state as InspectionArenaState;
  const presentation = shape(
    x.presentation,
    "contractVersion run tick simulationStep bosses floor",
  );
  requireValue(presentation.contractVersion === 1, "presentation contract");
  unsigned(presentation.run);
  unsigned(presentation.simulationStep);
  completedTick(presentation.tick);
  list(presentation.bosses, (v) => cue(v, true));
  nullable(presentation.floor, (v) => cue(v, false));
  requireValue(
    s.tick === presentation.tick &&
      s.tick === mode.tick &&
      s.simulationSteps === presentation.simulationStep &&
      x.run === presentation.run,
    "mixed state/presentation/context",
  );
  const arena = shape(x.arena, "minX maxX minY maxY playerRadius chest portal");
  requireValue(
    arena.minX === -10 &&
      arena.maxX === 10 &&
      arena.minY === -10 &&
      arena.maxY === 10 &&
      arena.playerRadius === 0.5,
    "Arena geometry",
  );
  const chest = shape(arena.chest, "x y interactionRadius");
  requireValue(
    chest.x === -3 && chest.y === 0 && chest.interactionRadius === 2,
    "chest geometry",
  );
  const portal = shape(arena.portal, "x y triggerRadius");
  requireValue(
    portal.x === 0 && portal.y === 7.5 && portal.triggerRadius === 1.25,
    "portal geometry",
  );
  const events = shape(
    x.events,
    "capacity byteLimit payloadByteLimit firstSequence lastSequence dropped omittedPayloads entries",
  );
  requireValue(
    events.capacity === 256 &&
      events.byteLimit === 2097152 &&
      events.payloadByteLimit === 65536,
    "event limits",
  );
  unsigned(events.dropped);
  unsigned(events.omittedPayloads);
  nullable(events.firstSequence, unsigned);
  nullable(events.lastSequence, unsigned);
  const entries = list(events.entries, event, 256) as InspectionEvent[];
  requireValue(
    events.firstSequence === (entries[0]?.sequence ?? null) &&
      events.lastSequence === (entries.at(-1)?.sequence ?? null),
    "event endpoints",
  );
  let eventBytes = 0;
  entries.forEach((entry, index) => {
    eventBytes += utf8.encode(JSON.stringify(entry)).length;
    requireValue(
      entry.epoch === x.epoch &&
        unsigned(entry.revision) <= unsigned(x.revision) &&
        (index === 0 ||
          unsigned(entries[index - 1].sequence) < unsigned(entry.sequence)),
      "event context/order",
    );
  });
  requireValue(eventBytes <= 2097152, "retained event bytes");
  const timing = shape(
    x.timing,
    "scope capacity budgetUs maxDurationUs totalSamples overBudgetSamples last samples meanUs p95Us maxUs",
  );
  requireValue(
    timing.scope === "hostUpdate" &&
      timing.capacity === 256 &&
      timing.budgetUs === 16666.666666666668 &&
      timing.maxDurationUs === 86400000000,
    "timing contract",
  );
  unsigned(timing.totalSamples);
  requireValue(
    unsigned(timing.overBudgetSamples) <= unsigned(timing.totalSamples),
    "timing totals",
  );
  const samples = list(timing.samples, sample, 256) as InspectionTimingSample[];
  nullable(timing.last, sample);
  const last = samples.at(-1);
  requireValue(
    last
      ? timing.last !== null &&
          Object.entries(last).every(
            ([key, value]) =>
              (timing.last as Record<string, unknown>)[key] === value,
          )
      : timing.last === null,
    "last timing sample",
  );
  requireValue(
    unsigned(timing.totalSamples) >= BigInt(samples.length),
    "timing count",
  );
  samples.forEach((v, i) =>
    requireValue(
      unsigned(v.attempt) <= unsigned(x.attempt) &&
        unsigned(v.revision) <= unsigned(x.revision) &&
        (i === 0 || unsigned(samples[i - 1].attempt) < unsigned(v.attempt)),
      "timing identity/order",
    ),
  );
  for (const key of ["meanUs", "p95Us", "maxUs"]) {
    if (!samples.length)
      requireValue(timing[key] === null, "empty timing statistics");
    else scalar(timing[key], 0, 86400000000);
  }
  if (!samples.length)
    requireValue(
      timing.totalSamples === "0" && timing.overBudgetSamples === "0",
      "initial timing counters",
    );
  return freeze(value as InspectionSnapshot);
}
export function decodeInspection(source: string): InspectionSnapshot {
  requireValue(
    utf8.encode(source).length <= INSPECTION_BYTES,
    "response byte limit",
  );
  return validateInspection(JSON.parse(source));
}
export function canAdopt(
  previous: InspectionSnapshot | null,
  next: InspectionSnapshot,
): boolean {
  return (
    !previous ||
    previous.sessionId !== next.sessionId ||
    (unsigned(next.revision) >= unsigned(previous.revision) &&
      unsigned(next.attempt) >= unsigned(previous.attempt) &&
      unsigned(next.epoch) >= unsigned(previous.epoch))
  );
}
export function contextKey(snapshot: InspectionSnapshot): string {
  return JSON.stringify([snapshot.sessionId, snapshot.epoch, snapshot.run]);
}
export type EntityKind =
  "player" | "enemy" | "projectile" | "grenade" | "effect";
export type InspectionEntity = {
  key: string;
  kind: EntityKind;
  id: number;
  label: string;
  position: Vec2;
  radius: number | null;
  health?: number;
  maxHealth?: number;
  raw: unknown;
};
export function entities(snapshot: InspectionSnapshot): InspectionEntity[] {
  const s = snapshot.state;
  const key = (kind: EntityKind, id: number) =>
    `${contextKey(snapshot)}:${kind}:${id}`;
  return [
    {
      key: key("player", 0),
      kind: "player",
      id: 0,
      label: "Player",
      position: s.playerPosition,
      radius: snapshot.arena.playerRadius,
      health: s.inventory.health,
      maxHealth: s.inventory.maxHealth,
      raw: {
        position: s.playerPosition,
        aim: s.aimDirection,
        inventory: s.inventory,
        weaponCooldowns: s.weaponCooldowns,
      },
    },
    ...s.enemies.map((v): InspectionEntity => ({
      key: key("enemy", v.Id),
      kind: "enemy",
      id: v.Id,
      label: `${["Pursuer", "Shooter", "Heavy", "Boss"][v.Kind] ?? "Enemy"} #${v.Id}`,
      position: v.Position,
      radius: v.Radius,
      health: v.Health,
      maxHealth: v.MaxHealth,
      raw: v,
    })),
    ...s.projectiles.map((v): InspectionEntity => ({
      key: key("projectile", v.Id),
      kind: "projectile",
      id: v.Id,
      label: `${v.EnemyOwned ? "Enemy" : "Player"} projectile #${v.Id}`,
      position: v.Position,
      radius: v.Radius,
      raw: v,
    })),
    ...s.grenades.map((v): InspectionEntity => ({
      key: key("grenade", v.Id),
      kind: "grenade",
      id: v.Id,
      label: `Grenade #${v.Id}`,
      position: v.Position,
      radius: null,
      raw: v,
    })),
    ...s.effects.map((v): InspectionEntity => ({
      key: key("effect", v.Id),
      kind: "effect",
      id: v.Id,
      label: `${["Slash", "Explosion", "Hit"][v.Kind] ?? "Effect"} #${v.Id}`,
      position: v.Position,
      radius: v.Radius,
      raw: v,
    })),
  ];
}
export function preserveSelection(
  key: string | null,
  snapshot: InspectionSnapshot,
): string | null {
  return key && entities(snapshot).some((entity) => entity.key === key)
    ? key
    : null;
}
export function mapPoint(point: Vec2, arena: ArenaGeometry): Vec2 {
  return {
    x: ((point.x - arena.minX) / (arena.maxX - arena.minX)) * 100,
    y: ((arena.maxY - point.y) / (arena.maxY - arena.minY)) * 100,
  };
}
export function mapAim(
  position: Vec2,
  direction: Vec2,
  arena: ArenaGeometry,
): Vec2 {
  return mapPoint(
    { x: position.x + direction.x, y: position.y + direction.y },
    arena,
  );
}
export function seekBody(tick: string, totalTicks: string): string {
  const target = completedTick(tick);
  requireValue(
    target === -1n || target < unsigned(totalTicks),
    "seek outside recording",
  );
  return `{"tick":${tick}}`;
}
export async function readBounded(
  response: Response,
  limit: number,
): Promise<string> {
  if (!response.body) return "";
  const reader = response.body.getReader();
  const parts: Uint8Array[] = [];
  let length = 0;
  try {
    while (true) {
      const { value, done } = await reader.read();
      if (done) break;
      length += value.byteLength;
      if (length > limit) {
        await reader.cancel();
        throw new Error("Arena response exceeds byte limit");
      }
      parts.push(value);
    }
  } finally {
    reader.releaseLock();
  }
  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const part of parts) {
    bytes.set(part, offset);
    offset += part.length;
  }
  return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
}
export async function replayAction(
  endpoint: string,
  action: "play" | "pause" | "step" | "stop" | "live" | "seek",
  tick?: string,
  totalTicks?: string,
  signal?: AbortSignal,
  fetcher: typeof fetch = fetch,
): Promise<void> {
  const body =
    action === "seek"
      ? seekBody(tick ?? "", totalTicks ?? "")
      : JSON.stringify({ action });
  const response = await fetcher(
    `${endpoint.replace(/\/$/, "")}/arena/playback/${action === "seek" ? "seek" : "command"}`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body,
      signal,
    },
  );
  if (!response.ok) {
    const result = await readBounded(response, 65536);
    let message = result;
    try {
      message = (JSON.parse(result) as { error?: string }).error ?? result;
    } catch {
      /* Retain plain HTTP error. */
    }
    throw new Error(message || response.statusText);
  }
  await response.body?.cancel(); // Acknowledgment only; the coherent GET confirms application.
}
export type InspectionView = {
  snapshot: InspectionSnapshot | null;
  status: "idle" | "loading" | "current" | "stale" | "hidden";
  error: string | null;
  updatedAt: number | null;
};
export type InspectionPollerOptions = {
  endpoint: string;
  changed: (view: InspectionView) => void;
  fetcher?: typeof fetch;
  delay?: (callback: () => void, ms: number) => ReturnType<typeof setTimeout>;
  cancelDelay?: (id: ReturnType<typeof setTimeout>) => void;
  now?: () => number;
};
export function createInspectionPoller(options: InspectionPollerOptions) {
  const fetcher = options.fetcher ?? fetch,
    delay = options.delay ?? setTimeout,
    cancelDelay = options.cancelDelay ?? clearTimeout,
    now = options.now ?? Date.now;
  let endpoint = options.endpoint,
    running = false,
    visible = true,
    generation = 0;
  let abort: AbortController | null = null,
    request: Promise<void> | null = null,
    timer: ReturnType<typeof setTimeout> | null = null;
  let view: InspectionView = {
    snapshot: null,
    status: "idle",
    error: null,
    updatedAt: null,
  };
  const emit = (changes: Partial<InspectionView>) => {
    view = { ...view, ...changes };
    options.changed(view);
  };
  const cancel = () => {
    generation++;
    abort?.abort();
    abort = null;
    request = null;
    if (timer !== null) cancelDelay(timer);
    timer = null;
  };
  const refresh = (): Promise<void> => {
    if (!running || !visible) return Promise.resolve();
    if (request) return request;
    if (timer !== null) cancelDelay(timer);
    timer = null;
    const id = generation,
      controller = new AbortController();
    abort = controller;
    let timedOut = false;
    const deadline = delay(() => {
      timedOut = true;
      controller.abort();
    }, 5000);
    if (!view.snapshot) emit({ status: "loading" });
    request = (async () => {
      try {
        const response = await fetcher(
          `${endpoint.replace(/\/$/, "")}/arena/inspection`,
          { signal: controller.signal, cache: "no-store" },
        );
        const source = await readBounded(response, INSPECTION_BYTES);
        if (!response.ok) {
          let message = response.statusText;
          try {
            message =
              (JSON.parse(source) as { error?: string }).error ?? message;
          } catch {
            /* Use HTTP status. */
          }
          throw new Error(message);
        }
        const next = decodeInspection(source);
        if (id !== generation || !running || !visible) return;
        if (!canAdopt(view.snapshot, next))
          throw new Error("Outdated Arena observation ignored");
        emit({
          snapshot: next,
          status: "current",
          error: null,
          updatedAt: now(),
        });
      } catch (error) {
        if (id === generation && running && visible)
          emit({
            status: "stale",
            error: timedOut
              ? "Arena inspection timed out"
              : error instanceof Error
                ? error.message
                : String(error),
          });
      } finally {
        cancelDelay(deadline);
        if (id === generation) {
          request = null;
          abort = null;
          if (running && visible)
            timer = delay(() => {
              timer = null;
              void refresh();
            }, 200);
        }
      }
    })();
    return request;
  };
  return {
    start() {
      if (running) return;
      running = true;
      void refresh();
    },
    refresh,
    stop() {
      running = false;
      cancel();
    },
    setVisible(value: boolean) {
      if (visible === value) return;
      visible = value;
      cancel();
      if (visible) {
        emit({ status: view.snapshot ? "stale" : "loading" });
        void refresh();
      } else emit({ status: "hidden" });
    },
    setEndpoint(value: string) {
      if (endpoint === value) return;
      endpoint = value;
      cancel();
      emit({ snapshot: null, status: "loading", error: null, updatedAt: null });
      void refresh();
    },
  };
}

export type ReplayAction = "play" | "pause" | "step" | "stop" | "live" | "seek";
export type ReplayIntent = {
  action: ReplayAction;
  sessionId: string;
  epoch: string;
  target: string | null;
  recordingId: string | null;
  startingTick: string;
  totalTicks: string;
};
export function replayIntent(
  snapshot: InspectionSnapshot,
  action: ReplayAction,
  tick?: string,
): ReplayIntent {
  requireValue(
    snapshot.mode.active && !snapshot.mode.seeking,
    "an active, ready replay is required",
  );
  let target: string | null = null;
  if (action === "step") {
    requireValue(!snapshot.mode.playing, "pause before stepping");
    target = (completedTick(snapshot.mode.tick) + 1n).toString();
    seekBody(target, snapshot.mode.totalTicks);
  }
  if (action === "seek") {
    target = tick ?? "";
    seekBody(target, snapshot.mode.totalTicks);
  }
  if (action === "stop") target = "-1";
  return {
    action,
    sessionId: snapshot.sessionId,
    epoch: snapshot.epoch,
    recordingId: snapshot.mode.recordingId,
    startingTick: snapshot.mode.tick,
    totalTicks: snapshot.mode.totalTicks,
    target,
  };
}
// Command HTTP responses acknowledge admission. Only an adopted inspection can
// confirm owner-clock work; newer context also prevents a stale success banner.
export function confirmReplay(
  intent: ReplayIntent,
  next: InspectionSnapshot,
): "waiting" | "confirmed" {
  if (next.sessionId !== intent.sessionId)
    throw new Error("Arena host changed during the replay command");
  if (next.mode.error) throw new Error(next.mode.error);
  if (intent.action === "live")
    return !next.mode.active && !next.mode.seeking ? "confirmed" : "waiting";
  if (!next.mode.active)
    throw new Error("Replay command was superseded by return to live");
  if (next.mode.recordingId !== intent.recordingId)
    throw new Error("Replay recording changed during the command");
  if (next.mode.seeking) return "waiting";
  const replaced = unsigned(next.epoch) > unsigned(intent.epoch);
  if (intent.action === "seek" || intent.action === "stop") {
    if (!replaced) return "waiting";
    if (next.mode.tick !== intent.target)
      throw new Error("Replay target was superseded by another operation");
    return "confirmed";
  }
  if (replaced) throw new Error("Replay context changed during the command");
  if (intent.action === "play") {
    const reachedEnd =
      completedTick(next.mode.tick) > completedTick(intent.startingTick) &&
      completedTick(next.mode.tick) + 1n === unsigned(intent.totalTicks);
    return next.mode.playing || reachedEnd ? "confirmed" : "waiting";
  }
  if (intent.action === "pause")
    return !next.mode.playing ? "confirmed" : "waiting";
  if (next.mode.tick === intent.target) return "confirmed";
  if (completedTick(next.mode.tick) > completedTick(intent.target))
    throw new Error(
      "Another replay operation advanced past the requested step",
    );
  return "waiting";
}

export type EventDescription = {
  summary: string;
  entity: { kind: "enemy" | "player"; id: number } | null;
};
// Read only explicitly i32 entity/damage fields from the embedded event document.
// The original messageJson is kept for detail; IDs/ticks are never reserialized.
export function describeEvent(event: InspectionEvent): EventDescription {
  const fallback = {
    summary:
      event.detail.kind === "run"
        ? "Run started · versions captured"
        : event.detail.kind === "oversize"
          ? "Payload omitted · size and hash retained"
          : "",
    entity: null,
  };
  if (event.detail.kind !== "message") return fallback;
  try {
    const wire = JSON.parse(event.detail.messageJson) as {
      args?: { type: string; value: unknown }[];
    };
    const arg = wire.args?.[0];
    if (arg?.type !== "str" || typeof arg.value !== "string") return fallback;
    const data: unknown = JSON.parse(arg.value);
    if (!data || typeof data !== "object" || Array.isArray(data))
      return fallback;
    const fields = data as Record<string, unknown>;
    const i32 = (key: string) =>
      typeof fields[key] === "number" &&
      Number.isInteger(fields[key]) &&
      fields[key] >= -2147483648 &&
      fields[key] <= 2147483647
        ? (fields[key] as number)
        : null;
    let id: number | null = null,
      kind: "enemy" | "player" = "enemy",
      summary = "";
    if (
      event.address === "/game/arena/damage" &&
      ["enemy", "player"].includes(fields.targetKind as string)
    ) {
      id = i32("targetId");
      kind = fields.targetKind as typeof kind;
      summary = `Damage ${i32("damage") ?? "?"} · HP ${i32("health") ?? "?"} · absorbed ${i32("absorbed") ?? "?"}`;
    }
    if (event.address === "/game/arena/death") {
      id = i32("entityId");
      kind = fields.kind === "player" ? "player" : "enemy";
      summary = "Death";
    }
    if (event.address === "/game/arena/attack") {
      id = i32("actorId");
      kind = id === 0 ? "player" : "enemy";
      summary = "Attack";
    }
    if (event.address === "/game/arena/script-fault") {
      id = i32("enemyId");
      summary = "Boss script fault";
    }
    if (!summary) return fallback;
    return {
      summary: `${summary}${id === null ? "" : ` · ${kind} #${id}`}`,
      entity: id === null ? null : { kind, id },
    };
  } catch {
    return fallback;
  }
}
export function eventEntity(
  event: InspectionEvent,
  snapshot: InspectionSnapshot,
): InspectionEntity | null {
  if (event.epoch !== snapshot.epoch || event.run !== snapshot.run) return null;
  const target = describeEvent(event).entity;
  return target
    ? (entities(snapshot).find(
        (entry) => entry.kind === target.kind && entry.id === target.id,
      ) ?? null)
    : null;
}
