// Toggles Git Glance as a pane on the right of the current tab: opened by a
// key as a 1px overlay that does its one job and closes itself (like the
// shipped pane-next mod). The pane is told which folder to start in, since
// once it has focus it can't see the terminal it was opened from.
import { View, div } from "gpui-kit";
import { spaces, panes, focused, props, run, closeOverlay } from "herdr";

function folder() {
  const f = focused();
  const pane = f?.pane ? panes().find((p) => p.pane === f.pane) : null;
  return pane?.cwd || f?.cwd || spaces().find((s) => s.active)?.folder || null;
}

export default class GitGlancePane extends View {
  init(_props, _cx) {
    const overlay = props()?.overlay;
    if (overlay) closeOverlay(overlay);
    const tab = focused()?.tab;
    const open = panes().find((p) => p.tab === tab && p.mod === "git-glance");
    if (open) run("pane.close", { pane: open.pane });
    else run("pane.split", { direction: "right", mod: "git-glance", props: { cwd: folder() } });
  }

  render(_cx) {
    return div();
  }
}
