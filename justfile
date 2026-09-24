# Build and link every plugin. Run this after any change.
dev: build link

# Run each plugin's manifest [[build]] commands (all plugins, or the named ones).
build *plugins:
    python3 scripts/plugins.py build {{plugins}}

# Link (or relink) plugins with herdr. Relinking is safe and refreshes the manifest.
link *plugins:
    python3 scripts/plugins.py link {{plugins}}

# Build and link a single plugin by its directory name under plugins/.
dev-one name: (build name) (link name)

# Run every test in the cargo workspace.
test:
    cargo test --workspace

# Run the same checks as the CI gate on pull requests.
ci:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
