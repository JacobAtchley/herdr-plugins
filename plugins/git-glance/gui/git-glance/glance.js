// Runs the git-glance binary, which owns every git decision; this side only
// renders what it prints. `bin` is a symlink to the cargo release build.
import { exec, appHome } from "herdr";

const BIN = `${appHome()}/mods/git-glance/bin`;

/** One CLI call. Resolves `{ ok: true, ... }` or `{ ok: false, error }`; never throws. */
export async function glance(cwd, args) {
  let r;
  try {
    r = await exec(BIN, [...args, "--cwd", cwd]);
  } catch (e) {
    return { ok: false, error: `can't run git-glance (build it with \`just\`): ${e}` };
  }
  try {
    return JSON.parse(r.stdout);
  } catch {
    return { ok: false, error: (r.stderr || r.stdout).trim() || `git-glance exited with ${r.code}` };
  }
}
