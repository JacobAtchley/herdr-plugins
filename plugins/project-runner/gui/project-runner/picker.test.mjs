// node --test: the runner's key, selection and ordering rules, without the app.
import { test } from "node:test";
import assert from "node:assert/strict";
import { Ordered, move, action, emptyMessage } from "./picker.js";

test("older results are dropped", () => {
  const o = new Ordered();
  const first = o.next(), second = o.next();
  assert.equal(o.accept(second), true);
  assert.equal(o.accept(first), false);
  assert.equal(o.accept(o.next()), true);
});

test("move wraps at both ends and is 0 for an empty list", () => {
  assert.equal(move(0, -1, 3), 2);
  assert.equal(move(2, 1, 3), 0);
  assert.equal(move(1, 1, 3), 2);
  assert.equal(move(0, 1, 0), 0);
  assert.equal(move(4, -1, 0), 0);
});

test("keys map like the TUI", () => {
  const want = {
    enter: "tab", "ctrl-v": "right", "ctrl-x": "down",
    escape: "close", "ctrl-c": "close",
    up: "up", "ctrl-p": "up", down: "down-move", "ctrl-n": "down-move",
    "ctrl-u": "clear", a: null, backspace: null,
  };
  for (const [key, act] of Object.entries(want)) assert.equal(action(key), act, key);
});

test("empty message tells no scripts from no matches", () => {
  assert.equal(emptyMessage(0), "no scripts in package.json");
  assert.equal(emptyMessage(3), "no matches");
});
