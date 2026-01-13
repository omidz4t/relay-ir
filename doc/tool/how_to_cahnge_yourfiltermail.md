# How to migrate from filtermail.py to madfilter (Rust)

This guide explains how to replace the Python-based `filtermail.py` with the Rust-based `madfilter` implementation for improved performance and reliability.

## Prerequisites

- Rust toolchain installed on the target server (or a compatible build environment).
- `cargo` and `rustc` available.

## 1. Prepare the Source Code

Ensure the `madfilter` source code is available on your local machine.

```bash
# From the project root
cd madfilter
```

## 2. Deploy to Server

### Option A: Remote Build (Recommended for compatibility)

1. Create a directory for the source code on the server:
   ```bash
   ssh your-server "mkdir -p ~/madfilter_src/src"
   ```

2. Copy the source files:
   ```bash
   scp Cargo.toml Cargo.lock your-server:~/madfilter_src/
   scp src/*.rs your-server:~/madfilter_src/src/
   ```

3. Build the binary on the server:
   ```bash
   ssh your-server "source \$HOME/.cargo/env && cd ~/madfilter_src && cargo build --release"
   ```

### Option B: Local Build and SCP

1. Build locally:
   ```bash
   cargo build --release
   ```

2. Copy the binary to the server:
   ```bash
   scp target/release/madfilter your-server:/tmp/
   ```

## 3. Install the Binary

Move the compiled binary to a system-wide location:

```bash
ssh your-server "sudo cp ~/madfilter_src/target/release/madfilter /usr/local/bin/madfilter"
ssh your-server "sudo chmod +x /usr/local/bin/madfilter"
```

## 4. Update Systemd Services

The `madfilter` serves both outgoing and incoming mail but requires a flag to distinguish the mode.

### Update Outgoing Service
Edit `/etc/systemd/system/filtermail.service`:
```ini
[Service]
ExecStart=/usr/local/bin/madfilter --config /home/chatmail/chatmail.ini --mode outgoing
```

### Update Incoming Service
Edit `/etc/systemd/system/filtermail-incoming.service`:
```ini
[Service]
ExecStart=/usr/local/bin/madfilter --config /home/chatmail/chatmail.ini --mode incoming
```

## 5. Reload and Restart

Apply the changes to systemd and restart the services:

```bash
ssh your-server "sudo systemctl daemon-reload"
ssh your-server "sudo systemctl restart filtermail.service filtermail-incoming.service"
```

## 6. Verification

Check the logs to ensure the filters are running correctly:

```bash
ssh your-server "journalctl -u filtermail.service -f"
ssh your-server "journalctl -u filtermail-incoming.service -f"
```

You can verify it is listening on the correct ports (default 10080 for outgoing, 10081 for incoming):
```bash
ssh your-server "ss -tlnp | grep madfilter"
```

## Rollback

To rollback, simply change the `ExecStart` lines back to point to the Python interpreter and `filtermail.py` script, then reload and restart.
