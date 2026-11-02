// The runner's key, selection and ordering rules, kept free of the app's
// modules so `node --test` can check them (picker.test.mjs). Mirrors the
// TUI's `App::handle_key` in src/app.rs.

/** Hands out increasing numbers and accepts a result only if it is newer
 *  than the last one accepted, so a slow older query can't replace a newer one. */
export class Ordered {
  constructor() {
    this.issued = 0;
    this.applied = 0;
  }

  next() {
    return ++this.issued;
  }

  /** A newer query is still on its way, so the list on screen is stale. */
  pending() {
    return this.issued > this.applied;
  }

  accept(n) {
    if (n < this.applied) return false;
    this.applied = n;
    return true;
  }
}

/** Moves the selection by `delta`, wrapping at either end. */
export function move(selected, delta, len) {
  if (len === 0) return 0;
  return (((selected + delta) % len) + len) % len;
}

const ACTIONS = {
  enter: "tab", "ctrl-v": "right", "ctrl-x": "down",
  escape: "close", "ctrl-c": "close",
  up: "up", "ctrl-p": "up", down: "down-move", "ctrl-n": "down-move",
  "ctrl-u": "clear",
};

/** What a keystroke does; null leaves it to the query input. */
export function action(key) {
  return ACTIONS[key] ?? null;
}

/** `total` is how many scripts package.json has. */
export function emptyMessage(total) {
  return total === 0 ? "no scripts in package.json" : "no matches";
}

/** The first row to show so `selected` stays among `rows` visible rows,
 *  moving as little as possible (the TUI's offset rule). */
export function scrollTo(selected, start, rows) {
  if (rows <= 0 || selected < start) return selected;
  if (selected >= start + rows) return selected - rows + 1;
  return start;
}
