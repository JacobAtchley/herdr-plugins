// Git Glance for the Herdr app: the same glance as the TUI popup, drawn
// natively. Every git action goes through the git-glance binary (glance.js);
// this file only keeps view state and renders the JSON it prints.
//
// Opens two ways: as a dialog (`mod.open:git-glance`, props().overlay set) or
// as a pane beside the shell (git-glance-pane splits it in with props().cwd).
import { View, div } from "gpui-kit";
import { v_flex, h_flex } from "gpui-base";
import { spaces, panes, focused, props, theme, watchFolder } from "herdr";
import { glance } from "./glance.js";

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
    this.refresh(cx);
    this.handle.focus();
  }

  remote() {
    return Boolean(activeSpace()?.machine);
  }

  /** Runs one CLI command and takes in whatever it returned. */
  call(args, cx) {
    if (!this.cwd || this.remote()) return;
    const cwd = this.cwd;
    this.loading = true;
    cx.spawn(async (cx) => {
      const r = await glance(cwd, args);
      this.loading = false;
      if (cwd !== this.cwd) return;
      if (r.ok) {
        if (r.snapshot) this.data = r;
        this.error = null;
        this.watch(r.root, cx);
      } else if (!this.data || args[0] === "status") {
        this.data = null;
        this.error = r.error;
      }
      cx.notify();
    });
  }

  refresh(cx) {
    this.call(["status"], cx);
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

  entryRow(entry, t) {
    const color = entry.section === "staged" ? t.ansi2 : entry.section === "conflict" ? t.ansi1 : entry.section === "untracked" ? t.dim : t.blocked;
    return h_flex().id(`e-${entry.section}-${entry.path}`).h(24).flex_none().px(10).gap(10).items_center().rounded(6)
      .hover((s) => s.bg(t.hover))
      .child(div().w(14).flex_none().text_color(color).child(entry.code))
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
      for (const entry of group) rows.push(this.entryRow(entry, t));
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
    return this.statusBody(t);
  }

  render(cx) {
    this.follow(cx);
    const t = theme();
    return v_flex().id("git-glance").size_full().min_h(0).bg(t.background).text_color(t.foreground).text_size(13)
      .track_focus(this.handle)
      .child(this.header(t))
      .child(this.body(t));
  }
}
