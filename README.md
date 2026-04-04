<p align="center"><strong>Codexpilot</strong> is a fork of OpenAI Codex with separate app state and GitHub Copilot support.</p>

---

## Quickstart

### Building and running Codexpilot

Build from source:

```shell
cd codex-rs
cargo build -p codex-cli --bin codexpilot
```

Run the forked CLI:

```shell
./target/debug/codexpilot
```

### Authentication

`codexpilot` supports its own local app state under `~/.codexpilot` and can authenticate separately from upstream `codex`.

## Docs

- [**Getting started**](./docs/getting-started.md)
- [**Authentication**](./docs/authentication.md)
- [**Configuration**](./docs/config.md)
- [**Installing & building**](./docs/install.md)
- [**Contributing**](./docs/contributing.md)

## Fork attribution

Codexpilot is an independent fork of OpenAI Codex. It is not presented as the official OpenAI Codex project.

This repository is licensed under the [Apache-2.0 License](LICENSE).
