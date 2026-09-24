# herdr-plugins

Custom [herdr](https://herdr.dev) plugins, built and linked from one repo.

## Dev loop

```sh
just          # build every plugin and link it with herdr
just test     # run all tests
just dev-one command-palette   # build + link one plugin
```

herdr launches plugin commands fresh each time, so after `just` the next
invocation uses the new build. Relinking is safe; it refreshes the manifest.

## Layout

- `crates/herdr-client` — typed client for herdr's socket API, shared by plugins.
- `plugins/<name>/herdr-plugin.toml` — one directory per plugin.
- `scripts/plugins.py` — runs each manifest's `[[build]]` commands and links it.

## Adding a plugin

1. Create `plugins/<name>/herdr-plugin.toml` (see the herdr plugin docs).
2. For a Rust plugin, add `plugins/<name>` to `members` in the root `Cargo.toml`
   and use `cargo build --release --target-dir target` as its build command.
3. Run `just`.
