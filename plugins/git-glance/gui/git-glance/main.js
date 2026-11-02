// Git Glance for the Herdr app: the same glance as the TUI popup, drawn
// natively. Every git action goes through the git-glance binary (glance.js);
// this file only keeps view state and renders the JSON it prints.
//
// Opens two ways: as a dialog (`mod.open:git-glance`, props().overlay set) or
// as a pane beside the shell (git-glance-pane splits it in with props().cwd).
import { View, div } from "gpui-kit";
import { v_flex, h_flex, InputState, Input } from "gpui-base";
import { spaces, panes, focused, props, theme, watchFolder, closeOverlay, run } from "herdr";
import { glance } from "./glance.js";
import { diff, diffRows, branches, refilter, stashes } from "./views.js";

const VIEWS = { diff, branches, stashes };

const SECTIONS = [
  ["conflict", "Conflicts"],
  ["staged", "Staged"],
  ["unstaged", "Changes"],
  ["untracked", "Untracked"],
];

/** The folder the person is working in: the focused terminal's, else null
 *  (a mod pane has focus). The space's own folder is the last resort. */
function terminalFolder() {
  const f = focused();
  if (!f) return null;
  const pane = f.pane ? panes().find((p) => p.pane === f.pane) : null;
  if (pane) return pane.terminal ? pane.cwd || f.cwd : null;
  return f.terminal ? f.cwd : null;
}

const activeSpace = () => spaces().find((s) => s.active);

export default class GitGlance extends View {
  init(_props, cx) {
    this.handle = cx.focus_handle();
    this.overlay = props()?.overlay ?? null;
    this.cwd = props()?.cwd || terminalFolder() || activeSpace()?.folder || null;
    this.data = null; // last successful response with a snapshot
    this.error = null;
    this.loading = false;
    this.watched = null;
    this.busy = false;
    this.selected = 0;
    this.footer = null; // { level: "info" | "error", text }
    this.prompt = null; // { purpose: "commit" | "stash" }
    this.view = { kind: "status" };
    this.height = 0;
    this.inputs = {
      commit: InputState.new({ placeholder: "Commit message" }),
      stash: InputState.new({ placeholder: "Stash message (optional)" }),
      branch: InputState.new({ placeholder: "Filter, or a new branch name" }),
    };
    this.inputs.commit.on("submit", (_e, cx) => this.submitPrompt(cx));
    this.inputs.stash.on("submit", (_e, cx) => this.submitPrompt(cx));
    this.inputs.branch.on("change", (_e, cx) => {
      if (this.view.kind !== "branches") return;
      this.view.query = this.inputs.branch.value();
      refilter(this.view);
      cx.notify();
    });
    this.inputs.branch.on("submit", (_e, cx) => this.view.kind === "branches" && branches.choose(this, cx));
    this.refresh(cx);
    this.handle.focus();
  }

  remote() {
    return Boolean(activeSpace()?.machine);
  }

  /** Runs one CLI command and takes in whatever it returned. Actions wait
   *  for each other; a refresh during an action is dropped (the action
   *  returns a fresh snapshot anyway). */
  call(args, cx, then) {
    if (!this.cwd || this.remote()) return;
    const refresh = args[0] === "status";
    if (this.busy) return;
    if (!refresh) {
      this.busy = true;
      this.footer = null;
    }
    const cwd = this.cwd;
    cx.spawn(async (cx) => {
      const r = await glance(cwd, args);
      if (!refresh) this.busy = false;
      if (cwd !== this.cwd) return;
      if (r.ok) {
        if (r.snapshot) this.setData(r);
        this.error = null;
        if (r.notice) this.footer = { level: "info", text: r.notice };
        this.watch(r.root, cx);
        then?.(r);
      } else if (refresh || !this.data) {
        this.data = null;
        this.error = r.error;
      } else {
        this.footer = { level: "error", text: r.error };
      }
      cx.notify();
    });
  }

  refresh(cx) {
    this.call(["status"], cx);
  }

  /** Keeps the selection on the same file across refreshes when it's still there. */
  setData(r) {
    const old = this.entries()[this.selected];
    this.data = r;
    const entries = this.entries();
    const same = old ? entries.findIndex((e) => e.section === old.section && e.path === old.path) : -1;
    this.selected = same >= 0 ? same : Math.min(this.selected, Math.max(0, entries.length - 1));
  }

  entries() {
    return this.data?.snapshot.entries ?? [];
  }

  /** Rows that fit the scrolling body (for paging the diff). */
  page() {
    return Math.max(1, Math.floor((this.height - 120) / 20));
  }

  back(cx) {
    this.view = { kind: "status" };
    this.handle.focus();
    cx.notify();
  }

  openDiff(cx) {
    const entry = this.entries()[this.selected];
    if (!entry) return;
    this.call(["diff", entry.path, "--section", entry.section], cx, (r) => {
      this.view = { kind: "diff", title: r.diff.title, rows: diffRows(r.diff.text), scroll: 0 };
    });
  }

  openBranches(cx) {
    this.call(["branches"], cx, (r) => {
      this.view = { kind: "branches", branches: r.branches, query: "", matches: [], selected: 0 };
      refilter(this.view);
      this.inputs.branch.set_value("");
      this.inputs.branch.focus();
    });
  }

  openStashes(cx) {
    this.call(["stashes"], cx, (r) => {
      this.view = { kind: "stashes", stashes: r.stashes, selected: 0, confirm: null };
    });
  }

  fail(text) {
    this.footer = { level: "error", text };
  }

  close() {
    if (this.overlay) return closeOverlay(this.overlay);
    const tab = focused()?.tab;
    const own = panes().find((p) => p.mod === "git-glance" && p.tab === tab && p.focused)
      ?? panes().find((p) => p.mod === "git-glance" && p.tab === tab);
    if (own) run("pane.close", { pane: own.pane });
  }

  toggleStage(index, cx) {
    const entry = this.entries()[index];
    if (!entry) return;
    this.selected = index;
    this.call([entry.section === "staged" ? "unstage" : "stage", entry.path], cx);
  }

  openPrompt(purpose, cx) {
    const snapshot = this.data?.snapshot;
    if (!snapshot) return;
    if (purpose === "commit" && !snapshot.has_staged) return this.fail("nothing staged to commit");
    if (purpose === "stash" && !snapshot.entries.length) return this.fail("no local changes to stash");
    this.prompt = { purpose };
    this.footer = null;
    this.inputs[purpose].set_value("");
    this.inputs[purpose].focus();
    cx.notify();
  }

  closePrompt(cx) {
    this.prompt = null;
    this.handle.focus();
    cx.notify();
  }

  submitPrompt(cx) {
    if (!this.prompt) return;
    const text = this.inputs[this.prompt.purpose].value().trim();
    if (this.prompt.purpose === "commit") {
      if (!text) return this.fail("commit message cannot be empty"), cx.notify();
      this.call(["commit", "--message", text], cx);
    } else {
      this.call(text ? ["stash-push", "--message", text] : ["stash-push"], cx);
    }
    this.closePrompt(cx);
  }

  key(e, cx) {
    const k = e.keystroke;
    if (this.view.kind !== "status") {
      if (VIEWS[this.view.kind].key(this, k, cx)) cx.stop_propagation();
      return;
    }
    if (this.prompt) {
      if (k === "escape") {
        this.closePrompt(cx);
        cx.stop_propagation();
      }
      return;
    }
    const last = Math.max(0, this.entries().length - 1);
    const move = (to) => { this.selected = Math.max(0, Math.min(last, to)); cx.notify(); };
    const snapshot = this.data?.snapshot;
    switch (k) {
      case "escape": case "q": this.close(); break;
      case "up": case "k": case "ctrl-p": move(this.selected - 1); break;
      case "down": case "j": case "ctrl-n": move(this.selected + 1); break;
      case "home": case "g": move(0); break;
      case "end": case "shift-g": move(last); break;
      case "r": this.refresh(cx); break;
      case "space": this.toggleStage(this.selected, cx); break;
      case "a": if (snapshot?.entries.length) this.call(["stage-all"], cx); break;
      case "u": if (snapshot?.has_staged) this.call(["unstage-all"], cx); break;
      case "c": this.openPrompt("commit", cx); break;
      case "shift-s": this.openPrompt("stash", cx); break;
      case "enter": case "d": this.openDiff(cx); break;
      case "b": this.openBranches(cx); break;
      case "z": this.openStashes(cx); break;
      default: return;
    }
    cx.stop_propagation();
  }

  /** Follows the repo on disk: one loop per root, ended when the root changes. */
  watch(root, cx) {
    if (!root || this.watched === root) return;
    this.watched = root;
    cx.spawn(async (cx) => {
      try {
        while (this.watched === root) {
          await watchFolder(root);
          if (this.watched === root) this.refresh(cx);
        }
      } catch {
        if (this.watched === root) this.watched = null;
      }
    });
  }

  /** As a pane, follow the terminal the person moves to; keep the last one
   *  while this pane (or another mod) has focus. */
  follow(cx) {
    if (this.overlay) return;
    const folder = terminalFolder();
    if (folder && folder !== this.cwd) {
      this.cwd = folder;
      this.data = null;
      this.error = null;
      this.watched = null;
      this.refresh(cx);
    }
  }

  header(t) {
    const d = this.data, b = d?.snapshot.branch;
    const track = b ? [b.ahead ? `↑${b.ahead}` : "", b.behind ? `↓${b.behind}` : ""].filter(Boolean).join(" ") : "";
    return h_flex().h(40).flex_none().px(14).gap(10).items_center().border_b(1).border_color(t.border)
      .child(div().font_semibold().child(d ? d.repo : "Git Glance"))
      .when(Boolean(b), (e) => e.child(div().text_color(t.accent).child(b.head ?? `detached ${b.oid ?? ""}`)))
      .when(Boolean(track), (e) => e.child(div().text_size(11).text_color(t.muted).child(track)))
      .child(div().flex_1())
      .when(Boolean(d?.snapshot.stashes), (e) => e.child(div().text_size(11).text_color(t.muted).child(`${d.snapshot.stashes} stashed`)));
  }

  entryRow(entry, index, t) {
    const color = entry.section === "staged" ? t.ansi2 : entry.section === "conflict" ? t.ansi1 : entry.section === "untracked" ? t.dim : t.blocked;
    const staged = entry.section === "staged";
    return h_flex().id(`e-${entry.section}-${entry.path}`).h(24).flex_none().px(10).gap(10).items_center().rounded(6)
      .cursor_pointer().when(index === this.selected, (e) => e.bg(t.selected)).hover((s) => s.bg(t.hover))
      .on_click((e, cx) => {
        this.selected = index;
        this.handle.focus();
        if (e.click_count >= 2) this.toggleStage(index, cx);
        cx.notify();
      })
      .child(div().id(`t-${entry.section}-${entry.path}`).w(18).flex_none().text_center().rounded(4).text_color(color)
        .hover((s) => s.bg(t.raised))
        .on_click((_e, cx) => { cx.stop_propagation(); this.toggleStage(index, cx); })
        .child(entry.code))
      .child(div().flex_1().min_w(0).truncate().child(entry.orig ? `${entry.orig} → ${entry.path}` : entry.path));
  }

  statusBody(t) {
    const entries = this.data.snapshot.entries;
    if (!entries.length) return div().p(14).text_color(t.dim).child("Nothing to commit, working tree clean");
    const rows = [];
    for (const [section, title] of SECTIONS) {
      const group = entries.filter((e) => e.section === section);
      if (!group.length) continue;
      rows.push(div().id(`s-${section}`).px(10).pt(8).pb(2).text_size(11).font_semibold().text_color(t.muted).child(`${title} (${group.length})`));
      for (const entry of group) rows.push(this.entryRow(entry, entries.indexOf(entry), t));
    }
    return v_flex().id("entries").flex_1().min_h(0).overflow_y_scrollbar().p(6).children(rows);
  }

  emptyState(t, text) {
    return div().flex_1().flex().items_center().justify_center().p(20).text_color(t.dim).child(text);
  }

  body(t) {
    if (this.remote()) return this.emptyState(t, "git glance works on local repos only");
    if (!this.cwd) return this.emptyState(t, "No folder: focus a terminal first");
    if (this.error) return this.emptyState(t, this.error);
    if (!this.data) return this.emptyState(t, "Loading…");
    if (this.view.kind !== "status") return VIEWS[this.view.kind].render(this, t);
    return this.statusBody(t);
  }

  button(id, label, enabled, action, t) {
    return div().id(id).h(26).px(10).flex().items_center().rounded(6).text_size(12)
      .bg(t.raised).text_color(enabled ? t.foreground : t.dim)
      .when(enabled, (e) => e.cursor_pointer().hover((s) => s.bg(t.hover)).on_click((_e, cx) => action(cx)))
      .child(label);
  }

  message(t) {
    return div().when(Boolean(this.footer), (e) => e.px(12).py(4).text_size(12)
      .text_color(this.footer.level === "error" ? t.ansi1 : t.ansi2).child(this.footer.text));
  }

  footerBar(t) {
    if (!this.data || this.view.kind !== "status") return div();
    const s = this.data.snapshot;
    if (this.prompt) {
      const commit = this.prompt.purpose === "commit";
      return h_flex().flex_none().p(8).gap(8).items_center().border_t(1).border_color(t.border)
        .child(div().text_size(12).text_color(t.muted).child(commit ? "Commit" : "Stash"))
        .child(div().id("prompt").flex_1().min_w(0).h(28).px(8).flex().items_center().rounded(6).border_1().border_color(t.accent)
          .on_mouse_down("left", (_e, cx) => cx.stop_propagation())
          .child(Input.new(this.inputs[this.prompt.purpose]).h_full().text_size(12).text_color(t.foreground)))
        .child(this.button("prompt-ok", commit ? "Commit" : "Stash", true, (cx) => this.submitPrompt(cx), t))
        .child(this.button("prompt-cancel", "Cancel", true, (cx) => this.closePrompt(cx), t));
    }
    return v_flex().flex_none().border_t(1).border_color(t.border)
      .child(h_flex().p(8).gap(6).items_center()
        .child(this.button("stage-all", "Stage all", s.entries.length > 0, (cx) => this.call(["stage-all"], cx), t))
        .child(this.button("unstage-all", "Unstage all", s.has_staged, (cx) => this.call(["unstage-all"], cx), t))
        .child(div().flex_1())
        .child(this.button("stash", "Stash…", s.entries.length > 0, (cx) => this.openPrompt("stash", cx), t))
        .child(this.button("commit", "Commit…", s.has_staged, (cx) => this.openPrompt("commit", cx), t))
        .child(div().text_size(11).text_color(t.dim).child("space stage · enter diff · c commit · S stash · b branches · z stashes")));
  }

  render(cx) {
    this.follow(cx);
    const t = theme();
    return v_flex().id("git-glance").size_full().min_h(0).bg(t.background).text_color(t.foreground).text_size(13)
      .track_focus(this.handle).on_key_down((e, cx) => this.key(e, cx))
      .on_size_change((size, cx) => { this.height = size.height; cx.notify(); })
      .child(this.header(t))
      .child(this.body(t))
      .child(this.message(t))
      .child(this.footerBar(t));
  }
}
