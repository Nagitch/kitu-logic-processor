import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  validateInspection,
  decodeInspection,
  unsigned,
  completedTick,
  seekBody,
  entities,
  preserveSelection,
  mapPoint,
  mapAim,
  canAdopt,
  createInspectionPoller,
  readBounded,
  replayAction,
  replayIntent,
  confirmReplay,
  describeEvent,
  eventEntity,
  type InspectionSnapshot,
  type InspectionView,
} from "./arena-inspection.ts";

function fixture(name = "live"): InspectionSnapshot {
  return JSON.parse(
    readFileSync(
      new URL(`./fixtures/inspection-${name}.json`, import.meta.url),
      "utf8",
    ),
  );
}
function response(value = fixture(), status = 200) {
  return new Response(JSON.stringify(value), { status });
}
function tick(snapshot: InspectionSnapshot, value: string) {
  snapshot.state.tick = value;
  snapshot.mode.tick = value;
  snapshot.presentation.tick = value;
}
const flush = async () => {
  for (let i = 0; i < 20; i++) await Promise.resolve();
};

test("all complete contract examples validate and become immutable", () => {
  for (const name of ["initial", "live", "replay"]) {
    const source = JSON.stringify(fixture(name));
    const snapshot = decodeInspection(source);
    assert.equal(snapshot.state.tick, snapshot.presentation.tick);
    assert.ok(Object.isFrozen(snapshot.state.inventory.backpack));
    assert.throws(() => snapshot.state.floor++);
  }
});
test("whole DTO validation refuses schema, scalar, actor casing and mixed context", () => {
  const edits: ((value: InspectionSnapshot) => void)[] = [
    (x) => Object.assign(x, { schemaVersion: 2 }),
    (x) => Object.assign(x, { unexpected: true }),
    (x) => (x.state.elapsed = Infinity),
    (x) => (x.state.inventory.health = 2147483648),
    (x) => (x.state.tick = "3"),
    (x) => (x.presentation.run = "9"),
    (x) => (x.presentation.simulationStep = "9"),
    (x) => (x.revision = "00"),
    (x) => (x.mode.tick = "-2"),
    (x) => x.events.entries.push({ ...x.events.entries[0] }),
    (x) => (x.timing.last = null),
    (x) => (x.arena.portal.triggerRadius = 2),
    (x) => Object.assign(x.state, { enemies: [{ id: 1 }] }),
  ];
  for (const edit of edits) {
    const source = fixture();
    edit(source);
    assert.throws(() => validateInspection(source));
  }
});
test("exact unsigned and completed tick bounds never use floating point", () => {
  for (const value of [
    "0",
    "9007199254740991",
    "9007199254740992",
    "9007199254740993",
    "18446744073709551615",
  ])
    assert.equal(unsigned(value), BigInt(value));
  assert.equal(completedTick("9223372036854775807"), 9223372036854775807n);
  for (const value of [
    "-0",
    "00",
    "+1",
    "1e3",
    "1.0",
    " 1",
    "1 ",
    "18446744073709551616",
    1,
  ])
    assert.throws(() => unsigned(value));
  for (const value of ["-2", "9223372036854775808"])
    assert.throws(() => completedTick(value));
});
test("Inspector seek emits exact numeric JSON tokens and rejects injection/out of range", () => {
  for (const value of ["-1", "0", "9007199254740993", "9223372036854775807"])
    assert.equal(seekBody(value, "18446744073709551615"), `{"tick":${value}}`);
  for (const value of [
    "-0",
    "00",
    "+2",
    "1e1",
    "1.5",
    '1},"action":"live"',
    "20",
    "9223372036854775808",
  ])
    assert.throws(() => seekBody(value, "20"));
});
test("freshness uses revision/attempt/epoch, permits equal IDs and backward seek", () => {
  const before = fixture("replay");
  const after = structuredClone(before);
  before.revision = "9007199254740993";
  before.attempt = "9007199254740995";
  after.revision = "9007199254740994";
  after.attempt = "9007199254740996";
  after.epoch = (BigInt(before.epoch) + 1n).toString();
  tick(after, "-1");
  assert.ok(canAdopt(before, after));
  assert.ok(canAdopt(after, after));
  assert.ok(!canAdopt(after, before));
  const failed = structuredClone(after);
  failed.attempt = "9007199254740997";
  assert.ok(canAdopt(after, failed));
  const other = fixture();
  other.sessionId = "other-host";
  assert.ok(canAdopt(after, other));
});
test("typed selection stays in the selected run/epoch and clears removed identities", () => {
  const source = fixture(),
    player = entities(source)[0];
  assert.equal(preserveSelection(player.key, source), player.key);
  for (const field of ["run", "epoch", "sessionId"] as const) {
    const next = structuredClone(source);
    next[field] += "1";
    assert.equal(preserveSelection(player.key, next), null);
  }
  assert.equal(preserveSelection("not-present", source), null);
  const next = structuredClone(source);
  next.state.enemies = [
    {
      Id: 1,
      Kind: 3,
      Position: { x: 1, y: 2 },
      Health: 100,
      MaxHealth: 100,
      Radius: 1,
      Speed: 1,
      Damage: 1,
      AttackInterval: 1,
      AttackRange: 1,
      AttackCooldown: 0,
      BossState: 1,
      PhaseRemaining: 1,
    },
  ];
  const enemy = entities(next).find((v) => v.kind === "enemy")!;
  assert.equal(preserveSelection(enemy.key, source), null);
});
test("map coordinates use authoritative ground-plane geometry and invert only Y", () => {
  const arena = fixture().arena;
  assert.deepEqual(mapPoint({ x: -10, y: 10 }, arena), { x: 0, y: 0 });
  assert.deepEqual(mapPoint({ x: 10, y: -10 }, arena), { x: 100, y: 100 });
  assert.deepEqual(mapPoint({ x: 0, y: 0 }, arena), { x: 50, y: 50 });
  assert.deepEqual(mapAim({ x: 0, y: 0 }, { x: 0, y: 1 }, arena), {
    x: 50,
    y: 45,
  });
});
test("event JSON remains exact text including wide OSC int64 values", () => {
  const source = fixture();
  const event = source.events.entries[0];
  event.address = "/ui/arena/command";
  event.detail = {
    kind: "message",
    messageJson:
      '{"address":"/ui/arena/command","args":[{"type":"int64","value":9007199254740993}]}',
  };
  const next = validateInspection(source);
  assert.equal(next.events.entries[0].detail.kind, "message");
  assert.ok(
    JSON.stringify(next.events.entries[0].detail).includes("9007199254740993"),
  );
});
test("return-live replacement may include a real Runtime step; idle cannot", () => {
  const source = fixture();
  const last = source.timing.samples.at(-1)!;
  last.outcome = "replacement";
  last.runtimeAdvanced = true;
  last.simulationAdvanced = true;
  source.timing.last = Object.fromEntries(
    Object.entries(last).toReversed(),
  ) as typeof last;
  validateInspection(source);
  const bad = structuredClone(source);
  bad.timing.last!.outcome = "idle";
  bad.timing.samples.at(-1)!.outcome = "idle";
  assert.throws(() => validateInspection(bad));
});
test("commands require coherent owner-applied confirmation, not HTTP acknowledgement", async () => {
  const before = fixture("replay");
  before.mode.playing = false;
  before.mode.seeking = false;
  tick(before, "-1");
  const intent = replayIntent(before, "step");
  assert.equal(confirmReplay(intent, before), "waiting");
  const after = structuredClone(before);
  tick(after, "0");
  assert.equal(confirmReplay(intent, after), "confirmed");
  after.mode.error = "proof mismatch";
  assert.throws(() => confirmReplay(intent, after), /proof mismatch/);
  after.mode.error = null;
  after.epoch = (unsigned(before.epoch) + 1n).toString();
  assert.throws(() => confirmReplay(intent, after), /context changed/);
  const seek = replayIntent(before, "seek", "0");
  assert.equal(confirmReplay(seek, before), "waiting");
  assert.equal(confirmReplay(seek, after), "confirmed");
  tick(after, "1");
  assert.throws(() => confirmReplay(seek, after), /superseded/);
  let body = "";
  await replayAction(
    "http://host",
    "seek",
    "9007199254740993",
    "18446744073709551615",
    undefined,
    async (_input, options) => {
      body = options?.body as string;
      return response();
    },
  );
  assert.equal(body, '{"tick":9007199254740993}');
});

function harness() {
  let nextId = 1;
  const timers = new Map<number, { callback: () => void; ms: number }>();
  const calls: {
    signal?: AbortSignal | null;
    resolve: (response: Response) => void;
    reject: (error: unknown) => void;
  }[] = [];
  const views: InspectionView[] = [];
  const poller = createInspectionPoller({
    endpoint: "http://one",
    changed: (view) => views.push(view),
    now: () => 123,
    fetcher: (_input, init) =>
      new Promise((resolve, reject) =>
        calls.push({ signal: init?.signal, resolve, reject }),
      ),
    delay: (callback, ms) => {
      const id = nextId++;
      timers.set(id, { callback, ms });
      return id as unknown as ReturnType<typeof setTimeout>;
    },
    cancelDelay: (id) => {
      timers.delete(id as unknown as number);
    },
  });
  const fire = (ms: number) => {
    const entry = [...timers].find(([, value]) => value.ms === ms);
    assert.ok(entry);
    timers.delete(entry[0]);
    entry[1].callback();
  };
  return { poller, calls, views, timers, fire };
}
test("polling is single-flight, delayed 200 ms and freezes one coherent result", async () => {
  const h = harness();
  h.poller.start();
  const a = h.poller.refresh(),
    b = h.poller.refresh();
  assert.equal(a, b);
  assert.equal(h.calls.length, 1);
  h.calls[0].resolve(response());
  await a;
  assert.equal(h.views.at(-1)?.status, "current");
  assert.equal(h.views.at(-1)?.updatedAt, 123);
  assert.ok(Object.isFrozen(h.views.at(-1)?.snapshot));
  h.fire(200);
  assert.equal(h.calls.length, 2);
  h.poller.stop();
});
test("hidden, changed endpoint and unmount reject late responses from old generations", async () => {
  const h = harness();
  h.poller.start();
  h.poller.setVisible(false);
  assert.ok(h.calls[0].signal?.aborted);
  h.calls[0].resolve(response());
  await flush();
  assert.equal(h.views.at(-1)?.status, "hidden");
  assert.equal(h.views.at(-1)?.snapshot, null);
  h.poller.setVisible(true);
  assert.equal(h.calls.length, 2);
  h.poller.setEndpoint("http://two");
  assert.equal(h.calls.length, 3);
  assert.ok(h.calls[1].signal?.aborted);
  const stale = fixture();
  stale.sessionId = "stale-host";
  h.calls[1].resolve(response(stale));
  h.calls[2].resolve(response());
  await flush();
  assert.equal(h.views.at(-1)?.snapshot?.sessionId, "example-arena-host");
  h.poller.refresh();
  const count = h.views.length;
  h.poller.stop();
  h.calls[3].resolve(response());
  await flush();
  assert.equal(h.views.length, count);
});
test("invalid/error snapshots leave last valid observation visibly stale", async () => {
  const h = harness();
  h.poller.start();
  h.calls[0].resolve(response());
  await h.poller.refresh();
  const previous = h.views.at(-1)!.snapshot;
  const pending = h.poller.refresh();
  h.calls[1].resolve(
    new Response('{"error":"projection unavailable"}', { status: 503 }),
  );
  await pending;
  assert.equal(h.views.at(-1)?.snapshot, previous);
  assert.equal(h.views.at(-1)?.status, "stale");
  assert.match(h.views.at(-1)!.error!, /projection unavailable/);
  h.poller.stop();
});
test("request deadline aborts a hung request and reports stale state", async () => {
  const h = harness();
  h.poller.start();
  const pending = h.poller.refresh();
  h.fire(5000);
  assert.ok(h.calls[0].signal?.aborted);
  h.calls[0].reject(new Error("aborted"));
  await pending;
  assert.equal(h.views.at(-1)?.error, "Arena inspection timed out");
  h.poller.stop();
});
test("response reader rejects over-limit bytes and invalid UTF-8", async () => {
  await assert.rejects(readBounded(new Response("12345"), 4), /byte limit/);
  await assert.rejects(
    readBounded(new Response(new Uint8Array([0xc0, 0xff])), 4),
  );
});

test("historical event references never select a reused entity in another run", () => {
  const source = fixture();
  const event = source.events.entries[0];
  event.address = "/game/arena/damage";
  event.detail = {
    kind: "message",
    messageJson: JSON.stringify({
      address: event.address,
      args: [
        {
          type: "str",
          value:
            '{"targetId":0,"targetKind":"player","damage":12,"health":88,"absorbed":0,"tick":9007199254740993}',
        },
      ],
    }),
  };
  assert.equal(
    describeEvent(event).summary,
    "Damage 12 · HP 88 · absorbed 0 · player #0",
  );
  assert.equal(eventEntity(event, source)?.kind, "player");
  const next = structuredClone(source);
  next.run = "2";
  assert.equal(eventEntity(event, next), null);
});

test("an empty recording exposes its valid initial snapshot without enabling step", () => {
  const source = fixture("initial");
  source.mode.active = true;
  source.mode.recordingId = "a".repeat(64);
  source.readOnly = true;
  source.mode.totalTicks = "0";
  assert.equal(validateInspection(source).mode.tick, "-1");
  assert.equal(seekBody("-1", "0"), '{"tick":-1}');
  assert.throws(() => replayIntent(source, "step"), /outside recording/);
  const bad = structuredClone(source);
  tick(bad, "0");
  assert.throws(() => validateInspection(bad), /replay range/);
});

test("Play confirms verified progress to EOF even if no poll observed playing=true", () => {
  const before = fixture("replay");
  before.mode.playing = false;
  before.mode.seeking = false;
  before.mode.totalTicks = "2";
  tick(before, "0");
  const intent = replayIntent(before, "play");
  assert.equal(confirmReplay(intent, before), "waiting");
  const after = structuredClone(before);
  tick(after, "1");
  after.attempt = (unsigned(before.attempt) + 1n).toString();
  assert.equal(confirmReplay(intent, after), "confirmed");
  const alreadyEnded = replayIntent(after, "play");
  assert.equal(confirmReplay(alreadyEnded, after), "waiting");
});

test("a competing recording load cannot confirm a seek or stop at the same target", () => {
  const before = fixture("replay");
  before.mode.playing = false;
  before.mode.seeking = false;
  for (const action of ["stop", "seek"] as const) {
    const intent = replayIntent(before, action, "-1");
    const next = structuredClone(before);
    next.mode.recordingId = "c".repeat(64);
    next.epoch = (unsigned(before.epoch) + 1n).toString();
    tick(next, "-1");
    assert.throws(() => confirmReplay(intent, next), /recording changed/);
  }
});
