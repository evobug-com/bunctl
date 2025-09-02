# bunctl 🚀

> **Production-grade process manager for Bun applications using systemd**

[![Version](https://img.shields.io/badge/version-3.0.0-blue.svg)](https://github.com/evobug-com/bunctl)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Bun](https://img.shields.io/badge/bun-%E2%89%A51.0.0-f472b6.svg)](https://bun.sh)
[![systemd](https://img.shields.io/badge/systemd-required-orange.svg)](https://systemd.io/)

> **No sugar-coating**: v3 is a **ground-up rewrite** focused on reliability.
> It drops a few legacy behaviors from 2.x to be simpler, safer and more
> predictable.

**bunctl** is a fast, minimal process manager that uses **systemd** for supervision and
jounald for logs. It is a pragmatic replacement for PM2 when you're deploying Bun
apps on Linux.

---

## 🔥 What changed in v3 (breaking)

- **Template unit**: services are now instances of a single template unit:
  `bunctl@<name>.service`. No more bespoke service files. (2.x used `bun-app-<name>`.)
- **Journald-only logging**: file-based log piping & rotation were removed. Use:
  `bunctl logs <name>` or `journalctl -u bunctl@<name>.service`.
- **No global boot helper**: auto-start is now standard systemd enable/disable per app:
  `bunctl enable <name>` / `bunctl disable <name>`.
- **No hardcoded SITES_DIR**: we use the current directory (or `--cwd`).
- **Env management is sane**: per-app env lives in a `.env`-style file
  (`/etc/bunctl/<name>.env` for system services or `~/.config/bunctl/env/<name>.env`
  for user services). `bunctl env set KEY=VAL` edits that file idempotently.

If you are migrating from 2.x:
1. `bunctl delete <old-name>` (stops & removes old service)
2. Re-init with v3: `bunctl init --name <name> --entry src/server.ts --autostart`
3. `bunctl start <name>`

---

## 🚀 Quick Start

```bash
# Install bunctl
sudo install -m 0755 bunctl /usr/local/bin/bunctl  # or curl | sh if you prefer

# From your app directory
cd /var/www/sites/my-app

# Initialize (auto-detects entry if possible)
bunctl init --name my-app --entry src/server.ts --port 3000 --autostart

# Start the app
bunctl start my-app

# View status / logs
bunctl status
bunctl logs my-app -n 200 -f
```

> Not running as root? bunctl will use **systemd user services**
> (`systemctl --user`). Everything works the same.

---

## 🎯 Features

- **Systemd-native supervision** (restarts, cgroups, resource limits)
- **Zero runtime bloat** (single shell script + systemd)
- **Journald logs** with consistent identifiers
- **JSON status** for automation (`bunctl status --json`)
- **Safe env management** via drop-in files
- **Security hardening** (`NoNewPrivileges`, `ProtectSystem=strict`, …)
- **Works with Bun or Node** (`--runtime bun|node`)

---

## 📚 CLI

```
bunctl [--user|--system] <command> [args]

init [--name NAME] [--entry FILE] [--port N] [--cwd DIR]
     [--memory 512M] [--cpu 50] [--runtime bun|node] [--autostart]
start <name>            stop <name>           restart <name>         reload <name>
enable <name>           disable <name>        delete <name>
status [--json] [name]  logs [name] [-f] [-n N]   env <name> (set KEY=VAL|unset KEY)
health <name>           list                  restart-all [--parallel] [--wait]
restart-group '<glob>'  update [name ...]
version                 install-completion
```

### Examples

```bash
# System service (root) with limits
sudo bunctl --system init --name api --entry src/server.ts --port 3000 --memory 1G --cpu 75 --autostart
sudo bunctl start api

# User service (non-root)
bunctl --user init --name worker --entry worker.ts
bunctl enable worker
bunctl restart worker

# Manage env
bunctl env worker set LOG_LEVEL=info
bunctl restart worker

# Machine-readable
bunctl status --json | jq .
```

---

## ⚙️ How it works

- A single template unit lives at:
    - **system**: `/etc/systemd/system/bunctl@.service`
    - **user**: `~/.config/systemd/user/bunctl@.service`
- Each app has:
    - a **drop‑in** with instance-specific settings (WorkingDirectory, limits):  
      `[unit-dir]/bunctl@<name>.service.d/10-bunctl.conf`
    - an **env file** with `CMD`, `PORT`, …:
        - system: `/etc/bunctl/<name>.env`
        - user: `~/.config/bunctl/env/<name>.env`

The template uses `/bin/sh -lc 'exec $CMD'` so you can set whatever start
command you need (default is `bun run <entry>`).

---

## 📦 Configuration reference

| Field        | Type   | Default | Notes |
|--------------|--------|---------|------|
| `name`       | string | `pwd`   | App identifier (`a-z0-9._-`) |
| `entry`      | string | auto    | Entry file, e.g. `src/server.ts` |
| `port`       | number | -       | Written to env as `PORT` |
| `cwd`        | string | `pwd`   | Working directory |
| `memory`     | string | `512M`  | Mapped to `MemoryMax` |
| `cpu`        | number | `50`    | Mapped to `CPUQuota` (percent) |
| `runtime`    | enum   | `bun`   | `bun` or `node` |
| `autostart`  | bool   | false   | Calls `systemctl enable` on init |
| `mode`       | enum   | auto    | `user`, `system` or `auto` |

> Values are stored in `~/.config/bunctl/apps/<name>.json` for your convenience.  
> You can also edit the drop-in/env files directly and run `bunctl update`.

---

## 🔍 Troubleshooting

- **“Bun executable not found”** – Install Bun and ensure it’s on PATH.
- **Service doesn’t reload** – `bunctl reload` sends `SIGUSR1`; if your app does not
  handle it, bunctl falls back to restart.
- **Unexpected permissions** – For system units, set a service user/group via:
  `bunctl init ... --user www-data --group www-data` (or edit the drop-in).
- **Logs are empty** – Use `bunctl logs <name> -n 200 -f`. v3 no longer writes to `logs/` files.

---

## 🆚 PM2 vs bunctl (honest take)

- If you want clustering, dashboards and a JavaScript-managed supervisor, use **PM2**.
- If you want **systemd** reliability with Bun speed and near-zero overhead, use **bunctl**.

---

## 📜 License

MIT – see [LICENSE](LICENSE).
