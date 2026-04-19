# CodexPilot

<p align="center"><strong>codexpilot - use OpenAI Codex models from your GitHub Copilot subscription in the best harness for Codex models.</strong></p>

```bash
npm install -g codexpilot
```

codexpilot is a minimal fork of `openai/codex`.

It keeps its own local state in `~/.codexpilot`, supports switching between OpenAI and GitHub Copilot, and can surface previous Codex sessions in `/resume`.

---

## Install

Launch it:

```bash
codexpilot
```

Build from source instead:

```bash
cd codex-rs
cargo build -p codex-cli --bin codexpilot
./target/debug/codexpilot
```

---

## What is CodexPilot?

CodexPilot is a separate CLI built on top of the official Codex codebase.

It keeps its own app identity, auth, config, and runtime state under `~/.codexpilot`.

It can also surface previous upstream Codex sessions in `/resume` without using upstream config as the active runtime config.

---

## Why CodexPilot exists

CodexPilot exists for developers who want:

- Codex-class usage through a GitHub Copilot subscription
- a separate local app home and fork identity
- the ability to switch between OpenAI and GitHub Copilot
- a way to resume older upstream Codex sessions from the fork

---

## Core features

- **GitHub Copilot support** for Codex-class usage in the terminal
- **Separate app home** under `~/.codexpilot`
- **Provider switching** between OpenAI and GitHub Copilot
- **Resume upstream sessions** from `.codex` via `/resume`
- **`From Codex` section** in the resume picker for upstream sessions
- **Separate runtime identity** from upstream Codex

---

## Quick usage

Start interactive mode:

```bash
codexpilot
```

Run with a prompt:

```bash
codexpilot "explain this repository"
```

Useful in-app flows:

- `/login` — authenticate
- `/model` — switch model/provider
- `/resume` — resume a previous session

---

## Authentication

CodexPilot keeps its own auth and config separate from upstream Codex.

- CodexPilot state: `~/.codexpilot`
- Upstream Codex state: `~/.codex`

That separation is intentional. CodexPilot should not accidentally inherit upstream config or runtime behavior. At the same time, the resume flow can intentionally surface upstream sessions when useful.

---

## Documentation

- [Getting started](./docs/getting-started.md)
- [Authentication](./docs/authentication.md)
- [Configuration](./docs/config.md)
- [Installing & building](./docs/install.md)
- [Contributing](./docs/contributing.md)

---

## Attribution

CodexPilot is an independent fork of OpenAI Codex. It is not the official OpenAI Codex project.

This repository is licensed under the [Apache-2.0 License](LICENSE).
