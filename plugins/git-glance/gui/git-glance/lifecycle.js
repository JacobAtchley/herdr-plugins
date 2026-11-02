// The mod's refresh and repo-following rules, kept free of the app's modules
// so `node --test` can check them (lifecycle.test.mjs).

/** Runs `task` at most once at a time; requests made while it runs collapse
 *  into one more run after it. `request(spawn)` hands the loop to `spawn`
 *  (the mod passes `(fn) => cx.spawn(fn)`), so every run gets a live cx. */
export class Coalescer {
  constructor(task) {
    this.task = task;
    this.running = false;
    this.dirty = false;
  }

  request(spawn) {
    if (this.running) {
      this.dirty = true;
      return;
    }
    this.running = true;
    spawn(async (cx) => {
      try {
        do {
          this.dirty = false;
          try {
            await this.task(cx);
          } catch {
            // A failed run reports through its own result; keep the loop alive.
          }
        } while (this.dirty);
      } finally {
        this.running = false;
      }
    });
  }
}

/** Hands out increasing numbers and accepts a result only if it is newer
 *  than the last one accepted, so a slow old snapshot can't replace a newer one. */
export class Ordered {
  constructor() {
    this.issued = 0;
    this.applied = 0;
  }

  next() {
    return ++this.issued;
  }

  accept(n) {
    if (n < this.applied) return false;
    this.applied = n;
    return true;
  }
}

/** One watch loop per repo root. Following the same root is a no-op; a new
 *  root starts a loop and every older one ends at its next wake. */
export class Watcher {
  constructor(wait) {
    this.wait = wait;
    this.root = null;
    this.generation = 0;
  }

  follow(root, cx, onChange) {
    if (!root || root === this.root) return;
    this.root = root;
    const generation = ++this.generation;
    const current = () => this.generation === generation;
    cx.spawn(async (cx) => {
      try {
        while (current()) {
          await this.wait(root);
          if (current()) onChange(cx);
        }
      } catch {
        if (current()) this.root = null;
      }
    });
  }
}

/** Everything that belongs to one repo; replaced whole when the pane follows
 *  a terminal into another, so no prompt, picker or selection carries over. */
export function freshContext(cwd) {
  return { cwd, data: null, error: null, footer: null, prompt: null, selected: 0, view: { kind: "status" } };
}
