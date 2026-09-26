# Git Glance Plugin — Design

Date: 2026-09-26
Status: Implemented

## Goal

A keypress opens a popup showing git state for the focused pane's repository,
with the everyday actions (stage, diff, commit, switch branch, stash) one key
away. It is a glance, not a full git client like lazygit: it opens instantly,
does one thing, and closes.

## Constraints

- It runs as a herdr `popup` plugin pane, like the command palette. The popup
  receives all input and closes when the process exits.
- The repository comes from `HERDR_PLUGIN_CONTEXT_JSON`, trying
  `focused_pane_cwd`, then `workspace_cwd`, then the process cwd.
- It shells out to the `git` CLI, with no libgit2. That keeps hooks, config,
  credential and signing behavior identical to the user's own git.

## Decisions

| Decision | Choice | Reason |
|---|---|---|
| Status source | `git status --porcelain=v2 --branch --show-stash -z` | One process gives branch, upstream, ahead/behind, stash count and entries in a stable, NUL-safe format. |
| Lazy loading | Branches, stashes and diffs load on demand | Opening costs two git processes (`rev-parse` and `status`). |
| Locks | `GIT_OPTIONAL_LOCKS=0` | `status` won't take the index lock that concurrent agents or editors need. |
| Prompts | `GIT_TERMINAL_PROMPT=0`, stdin null | Nothing can block waiting on input the popup can't show. |
| Untracked files | `-unormal` (directories collapsed) | Status stays cheap in trees with large untracked directories. A directory's "diff" lists its files. |
| Diff size | Capped at 512 KiB | Bounds memory for huge or generated files. |
| Branch filter | Case-insensitive substring | Branch lists are short. A fuzzy matcher would add a dependency without helping. |
| Unborn repos | Unstage with `git rm --cached` | `restore --staged` and `reset` need a HEAD. |

## Structure

- `status.rs` parses porcelain v2 into a `Snapshot`. It is pure and tested
  against fixtures.
- `git.rs` has one method per git operation on `Repo`. It is tested against
  temporary repositories.
- `app.rs` holds the view and mode state machine. Keys produce an `Op`, and
  `apply(Outcome)` feeds results back. It knows nothing about git or the
  terminal.
- `ops.rs` maps an `Op` to `Repo` calls. Every op that changes the repository
  ends with a fresh snapshot.
- `render.rs` draws with ratatui and is tested with `TestBackend`.

## Out of scope (possible follow-ups)

- Network operations (fetch, pull, push). They can be slow and need
  credentials, so they belong in a detached command with progress reporting.
- Hunk-level staging.
- Auto-refresh while the popup is open.
