// node --test: the mod's refresh and repo-following rules, without the app.
import { test } from "node:test";
import assert from "node:assert/strict";
import { Coalescer, Ordered, Watcher, freshContext } from "./lifecycle.js";

const now = (fn) => fn("cx");
const tick = () => new Promise((r) => setImmediate(r));

function deferred() {
  let resolve;
  const promise = new Promise((r) => (resolve = r));
  return { promise, resolve };
}

test("a burst of refresh requests runs once now and once more after", async () => {
  const runs = [];
  const gates = [];
  const c = new Coalescer((cx) => { runs.push(cx); const d = deferred(); gates.push(d); return d.promise; });
  c.request(now);
  c.request(now);
  c.request(now);
  c.request(now);
  assert.equal(runs.length, 1);
  gates[0].resolve();
  await tick();
  assert.equal(runs.length, 2);
  gates[1].resolve();
  await tick();
  assert.equal(runs.length, 2);
  c.request(now);
  assert.equal(runs.length, 3);
});

test("a failing refresh doesn't wedge the coalescer", async () => {
  let calls = 0;
  const c = new Coalescer(async () => { calls++; throw new Error("boom"); });
  c.request(now);
  await tick();
  c.request(now);
  await tick();
  assert.equal(calls, 2);
});

test("results older than the last applied one are refused", () => {
  const o = new Ordered();
  const first = o.next(), second = o.next();
  assert.equal(o.accept(second), true);
  assert.equal(o.accept(first), false);
  assert.equal(o.accept(o.next()), true);
});

test("following the same root again keeps one loop", async () => {
  const waits = [];
  const w = new Watcher((root) => { const d = deferred(); waits.push({ root, d }); return d.promise; });
  let changes = 0;
  w.follow("/repo", { spawn: now }, () => changes++);
  w.follow("/repo", { spawn: now }, () => changes++);
  await tick();
  assert.equal(waits.length, 1);
  waits[0].d.resolve();
  await tick();
  assert.equal(changes, 1);
  assert.equal(waits.length, 2);
});

test("a new root ends the old loop at its next wake", async () => {
  const waits = [];
  const w = new Watcher((root) => { const d = deferred(); waits.push({ root, d }); return d.promise; });
  const changed = [];
  w.follow("/a", { spawn: now }, () => changed.push("a"));
  w.follow("/b", { spawn: now }, () => changed.push("b"));
  w.follow("/a", { spawn: now }, () => changed.push("a2"));
  await tick();
  for (const { d } of waits.slice(0, 2)) d.resolve();
  await tick();
  // Only the latest loop reacts and keeps waiting.
  assert.deepEqual(changed, []);
  assert.deepEqual(waits.slice(3).map((x) => x.root), []);
  waits[2].d.resolve();
  await tick();
  assert.deepEqual(changed, ["a2"]);
  assert.deepEqual(waits.map((x) => x.root), ["/a", "/b", "/a", "/a"]);
});

test("a fresh context drops everything tied to the old repo", () => {
  const c = freshContext("/other");
  assert.deepEqual(c, {
    cwd: "/other",
    data: null,
    error: null,
    footer: null,
    prompt: null,
    selected: 0,
    view: { kind: "status" },
  });
  assert.notEqual(freshContext("/x").view, freshContext("/x").view);
});
