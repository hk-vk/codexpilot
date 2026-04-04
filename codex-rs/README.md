# Codexpilot CLI (Rust implementation)

We provide Codexpilot as a standalone native executable built from this workspace.

## Building Codexpilot

Build the forked binary from source:

```shell
cargo build -p codex-cli --bin codexpilot
./target/debug/codexpilot
```

## Documentation quickstart

- First run with Codexpilot? Start with [`docs/getting-started.md`](../docs/getting-started.md) for prompts, keyboard shortcuts, and session management.
- Want deeper control? See [`docs/config.md`](../docs/config.md) and [`docs/install.md`](../docs/install.md).

## What's new in the Rust CLI

This Rust implementation is the main Codexpilot experience. It includes features that the legacy TypeScript CLI never supported.

### Config

Codexpilot supports a rich set of configuration options. The Rust CLI uses `config.toml` instead of `config.json`. See [`docs/config.md`](../docs/config.md) for details.

### Model Context Protocol Support

#### MCP client

Codexpilot functions as an MCP client that allows the CLI to connect to MCP servers on startup. See the [`configuration documentation`](../docs/config.md#connecting-to-mcp-servers) for details.

#### MCP server (experimental)

Codexpilot can be launched as an MCP _server_ by running `codexpilot mcp-server`. This allows other MCP clients to use Codexpilot as a tool.

Use the [`@modelcontextprotocol/inspector`](https://github.com/modelcontextprotocol/inspector) to try it out:

```shell
npx @modelcontextprotocol/inspector codexpilot mcp-server
```

Use `codexpilot mcp` to add/list/get/remove MCP server launchers defined in `config.toml`, and `codexpilot mcp-server` to run the MCP server directly.

### Notifications

You can enable notifications by configuring a script that is run whenever the agent finishes a turn. The [notify documentation](../docs/config.md#notify) includes a detailed example that explains how to get desktop notifications via [terminal-notifier](https://github.com/julienXX/terminal-notifier) on macOS. When Codex detects that it is running under WSL 2 inside Windows Terminal (`WT_SESSION` is set), the TUI automatically falls back to native Windows toast notifications so approval prompts and completed turns surface even though Windows Terminal does not implement OSC 9.

### `codexpilot exec` to run Codexpilot non-interactively

To run Codexpilot non-interactively, run `codexpilot exec PROMPT` (you can also pass the prompt via `stdin`) and Codexpilot will work on your task until it decides that it is done and exits. If you provide both a prompt argument and piped stdin, Codexpilot appends stdin as a `<stdin>` block after the prompt so patterns like `echo "my output" | codexpilot exec "Summarize this concisely"` work naturally. Output is printed to the terminal directly. You can set the `RUST_LOG` environment variable to see more about what's going on.
Use `codexpilot exec --ephemeral ...` to run without persisting session rollout files to disk.

### Experimenting with the Codexpilot sandbox

To test what happens when a command runs under the sandbox provided by Codexpilot, use the following subcommands:

```
# macOS
codexpilot sandbox macos [--full-auto] [--log-denials] [COMMAND]...

# Linux
codexpilot sandbox linux [--full-auto] [COMMAND]...

# Windows
codexpilot sandbox windows [--full-auto] [COMMAND]...

# Legacy aliases
codex debug seatbelt [--full-auto] [--log-denials] [COMMAND]...
codex debug landlock [--full-auto] [COMMAND]...
```

### Selecting a sandbox policy via `--sandbox`

The Rust CLI exposes a dedicated `--sandbox` (`-s`) flag that lets you pick the sandbox policy **without** having to reach for the generic `-c/--config` option:

```shell
# Run Codexpilot with the default, read-only sandbox
codexpilot --sandbox read-only

# Allow the agent to write within the current workspace while still blocking network access
codexpilot --sandbox workspace-write

# Danger! Disable sandboxing entirely (only do this if you are already running in a container or other isolated env)
codex --sandbox danger-full-access
```

The same setting can be persisted in `~/.codexpilot/config.toml` via the top-level `sandbox_mode = "MODE"` key, e.g. `sandbox_mode = "workspace-write"`.
In `workspace-write`, Codexpilot also includes `~/.codexpilot/memories` in its writable roots so memory maintenance does not require an extra approval.

## Code Organization

This folder is the root of a Cargo workspace. It contains quite a bit of experimental code, but here are the key crates:

- [`core/`](./core) contains the business logic for the CLI. Ultimately, we hope this to be a library crate that is generally useful for building other Rust/native applications.
- [`exec/`](./exec) "headless" CLI for use in automation.
- [`tui/`](./tui) CLI that launches a fullscreen TUI built with [Ratatui](https://ratatui.rs/).
- [`cli/`](./cli) CLI multitool that provides the aforementioned CLIs via subcommands.

If you want to contribute or inspect behavior in detail, start by reading the module-level `README.md` files under each crate and run the project workspace from the top-level `codex-rs` directory so shared config, features, and build scripts stay aligned.
