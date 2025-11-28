# kubescope

A terminal UI for viewing Kubernetes deployment logs.

## Features

- Browse contexts, namespaces, and deployments
- Stream logs from multiple pods simultaneously
- Filter logs with regex patterns
- JSON log parsing with key filtering
- Keyboard-driven navigation

## Installation

```bash
cargo install --path .
```

## Usage

```
kubescope [OPTIONS] [CONTEXT] [NAMESPACE] [DEPLOYMENT]
```

### Arguments

| Argument | Description |
|----------|-------------|
| `CONTEXT` | Kubernetes context name (optional, will prompt if not provided) |
| `NAMESPACE` | Namespace (optional, requires context) |
| `DEPLOYMENT` | Deployment name (optional, requires context and namespace) |

### Options

| Option | Default | Description |
|--------|---------|-------------|
| `--buffer-size` | 10000 | Buffer size for log entries |
| `--tail-lines` | 100 | Number of historical log lines to fetch per pod |

### Examples

```bash
# Interactive mode - browse contexts, namespaces, deployments
kubescope

# Jump to namespace selection for a specific context
kubescope my-cluster

# Jump to deployment selection
kubescope my-cluster production

# Jump directly to logs for a deployment
kubescope my-cluster production my-app

# Stream with more history
kubescope my-cluster production my-app --tail-lines 500
```

## Keybindings

| Key | Action |
|-----|--------|
| `j/k` or `↓/↑` | Navigate lists / scroll logs |
| `Enter` | Select item |
| `Esc` | Go back |
| `/` | Search/filter logs |
| `r` / `R` | Cycle time range (5m, 15m, 30m, 1h, 6h, 24h, All) |
| `K` | Toggle JSON key filter |
| `t` | Toggle timestamps |
| `T` | Toggle local/UTC time |
| `p` | Toggle pod names |
| `f` | Toggle auto-scroll (follow mode) |
| `e` | Export logs to file |
| `?` | Show help |
| `q` | Quit |

## Building

```bash
cargo build --release
```

## License

MIT
