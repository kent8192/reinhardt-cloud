# reinhardt-cloud-cli

End-user command-line interface for the Reinhardt Cloud PaaS. Builds `ReinhardtApp` Kubernetes resources from your project's `Cargo.toml` features and applies them directly to Kubernetes when `--direct` is used.

## Quick links

- Full usage guide → [`docs/tools/cli.md`](../../docs/tools/cli.md)
- Crate API → [`docs.rs/reinhardt-cloud-cli`](https://docs.rs/reinhardt-cloud-cli)
- Project README → [`../../README.md`](../../README.md)

## Installation

```bash
cargo install reinhardt-cloud-cli
```

Or download a pre-built binary from the [releases page](https://github.com/kent8192/reinhardt-cloud/releases).

## Minimal invocation

```bash
cd my-reinhardt-app
reinhardt-cloud init
reinhardt-cloud deploy --dry-run
reinhardt-cloud deploy --direct
```

Supported subcommands: `init`, `sync`, `deploy`, `status`, `login`, `credentials`, `crd`.

See the [full guide](../../docs/tools/cli.md) for every flag, example, and troubleshooting note.

## License

BSL-1.1 — see the repository root `LICENSE`.
