// The views behind the status list: a diff, the branch picker and the stash
// list. Each has `render(app, t)` and `key(app, keystroke, cx)` (true when the
// key was used), mirroring the TUI's `diff_key`, `branches_key` and
// `stashes_key` in src/app.rs. State lives on `app.view`.
import { div } from "gpui-kit";
import { v_flex, h_flex, Input } from "gpui-base";
import { settings } from "herdr";

const ROW = 20;

const clamp = (n, lo, hi) => Math.max(lo, Math.min(hi, n));

/** Numbers each line once so a frame draws only the rows on screen. */
export function diffRows(text) {
  let old = 0, next = 0;
  return text.replace(/\n$/, "").split("\n").map((line) => {
    const hunk = /^@@ -(\d+)(?:,\d+)? \+(\d+)/.exec(line);
    if (hunk) { old = +hunk[1]; next = +hunk[2]; return { kind: "@", text: line }; }
    const kind = line[0];
    if (kind === "-" && !line.startsWith("---")) return { kind, number: old++, text: line.slice(1) || " " };
    if (kind === "+" && !line.startsWith("+++")) return { kind, number: next++, text: line.slice(1) || " " };
    if (kind === " ") { old++; return { kind, number: next++, text: line.slice(1) || " " }; }
    return { kind: "", text: line };
  });
}

export const diff = {
  render(app, t) {
    const v = app.view, page = app.page();
    const rows = v.rows.slice(v.scroll, v.scroll + page + 1).map(({ kind, number, text }) => {
      if (kind === "@") return div().h(ROW).flex_none().px(12).truncate().bg(t.raised).text_color(t.dim).child(text);
      return h_flex().h(ROW).flex_none().gap(10)
        .when(kind === "+", (e) => e.bg(t.ansi2 + "18")).when(kind === "-", (e) => e.bg(t.ansi1 + "18"))
        .child(div().w(36).flex_none().text_right().text_color(t.dim).child(number === undefined ? "" : String(number)))
        .child(div().flex_1().min_w(0).truncate().text_color(kind === "" ? t.muted : t.foreground).child(text));
    });
    return v_flex().id("diff").flex_1().min_h(0).overflow_hidden()
      .font_family(settings().font_family ?? "JetBrains Mono").text_size(11)
      .on_scroll_wheel((e, cx) => {
        const lines = e.delta_lines ? -e.delta_lines.y : -e.delta.y / ROW;
        diff.scroll(app, Math.round(lines) || Math.sign(lines), cx);
      })
      .child(h_flex().h(28).flex_none().px(12).items_center().border_b(1).border_color(t.border)
        .child(div().flex_1().truncate().font_semibold().child(v.title))
        .child(div().text_size(11).text_color(t.dim).child(`${Math.min(v.scroll + 1, v.rows.length)}/${v.rows.length} · esc back`)))
      .children(rows);
  },

  scroll(app, by, cx) {
    const v = app.view;
    v.scroll = clamp(v.scroll + by, 0, Math.max(0, v.rows.length - app.page()));
    cx.notify();
  },

  key(app, k, cx) {
    const page = app.page(), v = app.view;
    switch (k) {
      case "escape": case "q": case "enter": case "d": app.back(cx); return true;
      case "up": case "k": case "ctrl-p": diff.scroll(app, -1, cx); return true;
      case "down": case "j": case "ctrl-n": diff.scroll(app, 1, cx); return true;
      case "pageup": case "b": diff.scroll(app, -page, cx); return true;
      case "pagedown": case "space": diff.scroll(app, page, cx); return true;
      case "home": case "g": diff.scroll(app, -v.rows.length, cx); return true;
      case "end": case "shift-g": diff.scroll(app, v.rows.length, cx); return true;
    }
    return false;
  },
};

/** Case-insensitive substring, like the TUI: branch names are typed exactly. */
export function refilter(v) {
  const needle = v.query.toLowerCase();
  v.matches = v.branches.map((b, i) => [b, i]).filter(([b]) => b.name.toLowerCase().includes(needle)).map(([, i]) => i);
  v.selected = clamp(v.selected, 0, Math.max(0, v.matches.length - 1));
}

export const branches = {
  render(app, t) {
    const v = app.view, query = v.query.trim();
    const rows = v.matches.map((index, row) => {
      const b = v.branches[index];
      return h_flex().id(`b-${b.name}`).h(24).flex_none().px(10).gap(10).items_center().rounded(6).cursor_pointer()
        .when(row === v.selected, (e) => e.bg(t.selected)).hover((s) => s.bg(t.hover))
        .on_click((_e, cx) => { v.selected = row; branches.choose(app, cx); })
        .child(div().w(10).flex_none().text_color(t.accent).child(b.current ? "*" : ""))
        .child(div().flex_1().min_w(0).truncate().text_color(b.current ? t.accent : t.foreground).child(b.name))
        .child(div().text_size(11).text_color(t.dim).child(b.age))
        .child(div().text_size(11).text_color(t.muted).child(b.track));
    });
    const hint = v.matches.length
      ? "enter switch · type to filter · esc back"
      : query ? `enter creates branch "${query}" · esc back` : "type a branch name · esc back";
    return v_flex().flex_1().min_h(0)
      .child(h_flex().p(8).gap(8).items_center().border_b(1).border_color(t.border)
        .child(div().text_size(12).text_color(t.muted).child("Branch"))
        .child(div().id("branch-filter").flex_1().min_w(0).h(28).px(8).flex().items_center().rounded(6).border_1().border_color(t.accent)
          .on_mouse_down("left", (_e, cx) => cx.stop_propagation())
          .child(Input.new(app.inputs.branch).h_full().text_size(12).text_color(t.foreground))))
      .child(v_flex().id("branches").flex_1().min_h(0).overflow_y_scrollbar().p(6).children(rows))
      .child(div().px(12).pb(6).text_size(11).text_color(t.dim).child(hint));
  },

  /** Enter on a match switches (the current one just goes back); with no
   *  match, a non-empty query creates that branch. */
  choose(app, cx) {
    const v = app.view, index = v.matches[v.selected];
    if (index !== undefined) {
      const b = v.branches[index];
      if (b.current) return app.back(cx);
      return app.call(["switch", b.name], cx, () => app.back(cx));
    }
    const name = v.query.trim();
    if (name) app.call(["switch", name, "--create"], cx, () => app.back(cx));
  },

  key(app, k, cx) {
    const v = app.view, last = Math.max(0, v.matches.length - 1);
    switch (k) {
      case "escape": app.back(cx); return true;
      case "up": case "ctrl-p": v.selected = clamp(v.selected - 1, 0, last); cx.notify(); return true;
      case "down": case "ctrl-n": v.selected = clamp(v.selected + 1, 0, last); cx.notify(); return true;
    }
    // Enter arrives as the filter input's submit; everything else is typing.
    return false;
  },
};

export const stashes = {
  render(app, t) {
    const v = app.view;
    const rows = v.stashes.map((s, row) =>
      h_flex().id(`z-${s.index}`).h(24).flex_none().px(10).gap(10).items_center().rounded(6).cursor_pointer()
        .when(row === v.selected, (e) => e.bg(t.selected)).hover((st) => st.bg(t.hover))
        .on_click((e, cx) => { v.selected = row; if (e.click_count >= 2) stashes.act(app, "pop", cx); cx.notify(); })
        .child(div().w(70).flex_none().text_color(t.accent).child(`stash@{${s.index}}`))
        .child(div().flex_1().min_w(0).truncate().child(s.subject))
        .child(div().text_size(11).text_color(t.dim).child(s.age)));
    const target = v.stashes[v.selected];
    return v_flex().flex_1().min_h(0)
      .child(h_flex().h(28).flex_none().px(12).items_center().border_b(1).border_color(t.border)
        .child(div().flex_1().font_semibold().child("Stashes")))
      .child(v.stashes.length
        ? v_flex().id("stashes").flex_1().min_h(0).overflow_y_scrollbar().p(6).children(rows)
        : div().flex_1().p(14).text_color(t.dim).child("No stashes"))
      .child(v.confirm !== null
        ? h_flex().p(8).gap(8).items_center().border_t(1).border_color(t.border)
          .child(div().flex_1().text_size(12).text_color(t.ansi1).child(`Drop stash@{${v.confirm}}? It cannot be recovered from here.`))
          .child(app.button("drop-yes", "Drop (y)", true, (cx) => stashes.drop(app, cx), t))
          .child(app.button("drop-no", "Cancel (n)", true, (cx) => { v.confirm = null; cx.notify(); }, t))
        : h_flex().p(8).gap(6).items_center().border_t(1).border_color(t.border)
          .child(app.button("pop", "Pop", Boolean(target), (cx) => stashes.act(app, "pop", cx), t))
          .child(app.button("apply", "Apply", Boolean(target), (cx) => stashes.act(app, "apply", cx), t))
          .child(app.button("drop", "Drop…", Boolean(target), (cx) => stashes.act(app, "drop", cx), t))
          .child(div().flex_1())
          .child(div().text_size(11).text_color(t.dim).child("enter/p pop · a apply · x drop · esc back")));
  },

  act(app, verb, cx) {
    const v = app.view, target = v.stashes[v.selected];
    if (!target) return;
    if (verb === "drop") { v.confirm = target.index; cx.notify(); return; }
    app.call(["stash", verb, String(target.index)], cx, () => app.back(cx));
  },

  drop(app, cx) {
    const v = app.view, index = v.confirm;
    v.confirm = null;
    // Dropping changes nothing on disk, so the list stays open with what's left.
    app.call(["stash", "drop", String(index)], cx, (r) => {
      v.stashes = r.stashes;
      v.selected = clamp(v.selected, 0, Math.max(0, r.stashes.length - 1));
    });
  },

  key(app, k, cx) {
    const v = app.view, last = Math.max(0, v.stashes.length - 1);
    if (v.confirm !== null) {
      if (k === "enter" || k === "y") stashes.drop(app, cx);
      else if (k === "escape" || k === "n") { v.confirm = null; cx.notify(); }
      return true;
    }
    switch (k) {
      case "escape": case "q": app.back(cx); return true;
      case "up": case "k": case "ctrl-p": v.selected = clamp(v.selected - 1, 0, last); cx.notify(); return true;
      case "down": case "j": case "ctrl-n": v.selected = clamp(v.selected + 1, 0, last); cx.notify(); return true;
      case "enter": case "p": stashes.act(app, "pop", cx); return true;
      case "a": stashes.act(app, "apply", cx); return true;
      case "x": stashes.act(app, "drop", cx); return true;
    }
    return false;
  },
};
