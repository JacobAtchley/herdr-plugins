// Project runner in the Herdr app: the package.json scripts of the focused
// terminal's project. Type to filter, enter runs in a new tab, ctrl-v / ctrl-x
// in a split, esc closes. Same rules and keys as the TUI (src/app.rs); the
// binary finds the project and ranks the scripts, this side draws them and
// launches with the app's own commands.
import { View, div } from "gpui-kit";
import { v_flex, h_flex, InputState, Input } from "gpui-base";
import { spaces, panes, focused, props, theme, run, closeOverlay, settings } from "herdr";
import { scripts } from "./runner.js";
import { Ordered, move, action, emptyMessage } from "./picker.js";

const ROW = 24;

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

export default class ProjectRunner extends View {
  init(_props, cx) {
    this.handle = cx.focus_handle();
    this.cwd = terminalFolder() || activeSpace()?.folder || null;
    // The last answer: { ok: true, project, total, scripts } or { ok: false, error }.
    this.data = null;
    this.selected = 0;
    this.error = null;
    this.order = new Ordered();
    this.query = InputState.new({ placeholder: "Filter scripts" });
    this.query.on("change", (_e, cx) => this.load(cx));
    this.query.on("submit", (_e, cx) => this.launch("tab", cx));
    this.load(cx);
    this.query.focus();
  }

  remote() {
    return Boolean(activeSpace()?.machine);
  }

  close() {
    closeOverlay(props()?.overlay);
  }

  /** Asks the binary for the current query; an answer older than one already
   *  shown is dropped. Every change puts the selection back on top. */
  load(cx) {
    if (!this.cwd || this.remote()) return;
    const n = this.order.next(), query = this.query.value();
    cx.spawn(async (cx) => {
      const r = await scripts(this.cwd, query);
      if (!this.order.accept(n)) return;
      this.data = r;
      this.selected = 0;
      this.error = null;
      cx.notify();
    });
  }

  list() {
    return this.data?.ok ? this.data.scripts : [];
  }

  /** `place` is "tab", "right" or "down". The popup closes once the app has
   *  taken the command; what goes wrong later shows as the app's own toast. */
  launch(place, cx) {
    const script = this.list()[this.selected];
    // Enter can arrive both as the input's submit and as a key: launch once.
    if (!script || this.launched) return;
    const folder = this.data.project.dir;
    try {
      if (place === "tab") {
        run("tab.new", { name: script.name, folder, argv: script.argv });
      } else {
        if (!focused()?.pane) {
          this.error = "focus a pane to split";
          return cx.notify();
        }
        run("terminal.open", { argv: script.split_argv, folder, place, name: script.name });
      }
    } catch (e) {
      this.error = String(e?.message ?? e);
      return cx.notify();
    }
    this.launched = true;
    this.close();
  }

  key(e, cx) {
    // Without a project there's only a message: any key closes, as in the TUI.
    if (this.data && !this.data.ok) {
      this.close();
      return cx.stop_propagation();
    }
    const act = action(e.keystroke);
    if (!act) return;
    cx.stop_propagation();
    const len = this.list().length;
    switch (act) {
      case "close": return this.close();
      case "up": this.selected = move(this.selected, -1, len); break;
      case "down-move": this.selected = move(this.selected, 1, len); break;
      case "clear": this.query.set_value(""); this.load(cx); break;
      default: return this.launch(act, cx);
    }
    cx.notify();
  }

  row(script, i, t) {
    const selected = i === this.selected;
    return h_flex().id(`s-${script.index}`).h(ROW).flex_none().px(10).gap(12).items_center().rounded(6).cursor_pointer()
      .when(selected, (e) => e.bg(t.selected)).hover((s) => s.bg(t.hover))
      .on_click((e, cx) => {
        if (e.click_count >= 2) return this.launch("tab", cx);
        this.selected = i;
        cx.notify();
      })
      .child(h_flex().w(220).flex_none().min_w(0).overflow_hidden()
        .children(script.segments.map(({ text, hit }) =>
          div().flex_none().when(hit, (e) => e.text_color(t.accent).font_bold()).child(text))))
      .child(div().flex_1().min_w(0).truncate().text_color(t.dim).child(script.command));
  }

  message(t, text) {
    return div().flex_1().p(14).text_color(t.dim).child(text);
  }

  body(t) {
    if (this.remote()) return this.message(t, "project runner works on local folders only");
    if (!this.cwd) return this.message(t, "No folder: focus a terminal first");
    if (!this.data) return this.message(t, "Loading…");
    if (!this.data.ok) {
      return v_flex().flex_1().p(14).gap(8)
        .child(div().text_color(t.foreground).child(this.data.error))
        .child(div().text_color(t.dim).child("press any key"));
    }
    const list = this.list();
    if (!list.length) return this.message(t, emptyMessage(this.data.total));
    return v_flex().id("scripts").flex_1().min_h(0).overflow_y_scrollbar().p(6)
      .children(list.map((s, i) => this.row(s, i, t)));
  }

  footer(t) {
    const p = this.data?.ok ? this.data.project : null;
    const right = this.error
      ? div().text_color(t.ansi1).child(this.error)
      : div().text_color(t.dim).child(p ? `${p.name} · ${p.manager}` : "");
    return h_flex().flex_none().px(12).py(6).gap(12).items_center().border_t(1).border_color(t.border).text_size(11)
      .child(div().text_color(t.dim).child("⏎ tab  ^v split right  ^x split down  esc close"))
      .child(div().flex_1())
      .child(right);
  }

  render(_cx) {
    const t = theme();
    return v_flex().id("project-runner").size_full().min_h(0).bg(t.background).text_color(t.foreground).text_size(13)
      .font_family(settings().font_family ?? "JetBrains Mono")
      .track_focus(this.handle).on_key_down((e, cx) => this.key(e, cx))
      .child(h_flex().flex_none().p(8).gap(8).items_center().border_b(1).border_color(t.border)
        .child(div().text_color(t.accent).font_bold().child("❯"))
        .child(div().flex_1().min_w(0).h(26).flex().items_center()
          .child(Input.new(this.query).h_full().text_size(13).text_color(t.foreground))))
      .child(this.body(t))
      .child(this.footer(t));
  }
}
