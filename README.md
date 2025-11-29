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
kubescope init
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
| `-e`, `--filter` | | Regex pattern to pre-populate log filter |
| `-i`, `--ignore-case` | false | Case insensitive filter matching |
| `-v`, `--invert-match` | false | Invert filter match (show non-matching lines) |
| `--no-config` | false | Ignore `.kubescope` config file |

## Configuration File

Create a `.kubescope` file in your project directory to automatically load settings when running kubescope.

### Creating a Config File

Use the interactive `init` command:

```bash
kubescope init
```

This walks you through selecting a context, namespace, deployment, and filter pattern.

### Manual Configuration

Create a `.kubescope` file manually with any of these options:

```toml
# Kubernetes context name
context = "my-cluster"

# Namespace
namespace = "production"

# Deployment name
deployment = "my-app"

# Filter pattern (regex)
filter = "error|warn"

# Case insensitive matching
ignore_case = true

# Invert match (show non-matching lines)
invert_match = false

# Buffer size for log entries
buffer_size = 10000

# Historical log lines per pod
tail_lines = 100
```

All fields are optional. CLI arguments override config file values.

### Ignoring the Config File

```bash
kubescope --no-config
```

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

# Filter logs for errors (grep-like syntax)
kubescope my-cluster production my-app -e "error|exception"

# Case insensitive filter
kubescope my-cluster production my-app -e "ERROR" -i

# Exclude health check logs
kubescope my-cluster production my-app -e "health.check" -v

# Combined: case insensitive error filter
kubescope my-cluster production my-app -e "error" -i

# Initialize a .kubescope config file
kubescope init

# Run with config file (auto-loaded from .kubescope)
kubescope

# Override config file settings with CLI args
kubescope other-context
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
