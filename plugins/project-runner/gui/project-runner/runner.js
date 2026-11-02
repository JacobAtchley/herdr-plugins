// Runs the project-runner binary, which owns discovery, the package manager
// and ranking; this side only renders what it prints. `bin` is a symlink to
// the cargo release build.
import { exec, appHome } from "herdr";

const BIN = `${appHome()}/mods/project-runner/bin`;

/** The project at `cwd` and its scripts ranked for `query`. Resolves
 *  `{ ok: true, project, total, scripts }` or `{ ok: false, error }`; never throws. */
export async function scripts(cwd, query) {
  let r;
  try {
    r = await exec(BIN, ["scripts", "--cwd", cwd, "--query", query]);
  } catch (e) {
    return { ok: false, error: `can't run project-runner (build it with \`just\`): ${e}` };
  }
  try {
    return JSON.parse(r.stdout);
  } catch {
    return { ok: false, error: (r.stderr || r.stdout).trim() || `project-runner exited with ${r.code}` };
  }
}
