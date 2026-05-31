# Connections and Forwards

Use the Sessions, Connection Pool, Connection Monitor, and forwarding surfaces for normal SSH work. The CLI companion is only for headless validation, export, and repeatable setup.

## Saved Connections

Saved connections hold reusable SSH profile data: name, host, user, port, group, tags, color, authentication mode, and optional post-connect command.

Create and edit saved connections from the connection manager or Sessions view. For a new host, fill in the profile, choose an authentication mode, save it, then open the connection from Sessions. If the connection fails, edit the same saved profile instead of creating duplicate entries with similar labels.

Use groups, colors, and tags for navigation. Do not put passwords, tokens, or environment secrets in names, groups, tags, notes, or post-connect labels.

## Connection Runtime

Saved profiles and live runtime nodes are different things:

- Saved profile: the host and connection settings OxideTerm should use.
- SSH node: the live or reconnecting runtime state for a host.
- Terminal session: a visible shell attached to an SSH node.
- SFTP session: a file browsing or transfer surface attached to an SSH node.

Use Connection Pool and Connection Monitor when a terminal looks stuck, SFTP cannot read a directory, or reconnect behavior is unclear. Reconnect the runtime from the app state; do not delete and recreate the saved profile just to reconnect.

## Connecting

Typical flow:

1. Open Sessions.
2. Select a saved connection or create one.
3. Open the connection.
4. Wait for the SSH node and terminal tab to become live.
5. If needed, open SFTP, IDE, or forwarding from the same connected node.

For unstable hosts, keep Connection Monitor open while testing. It shows whether a node is connected, connecting, stale, or unavailable.

## Serial Terminals

Serial terminals are local device transports, not SSH subfeatures. Use them for USB-UART adapters, development boards, router consoles, switch consoles, or other local serial devices.

Open a serial terminal from the New Connection dialog by selecting the `Serial` branch. Fill in `Serial port`, `Baud rate`, `Data bits`, `Stop bits`, `Parity`, and `Flow control`, then choose `Open Serial`. The command palette also exposes `Open Serial Terminal`.

Common port names:

| Platform | Examples |
|----------|----------|
| macOS | `/dev/cu.usbserial-0001`, `/dev/cu.usbmodem*` |
| Linux | `/dev/ttyUSB0`, `/dev/ttyACM0` |
| Windows | `COM3`, `COM10` |

Use `Save serial profile` and `Profile name` when the same device settings should be reused. Saved serial profiles are separate from saved SSH connections.

Serial terminals do not provide SFTP, port forwarding, ProxyJump, SSH host-key verification, SSH Agent, remote IDE, or SSH connection-pool behavior. Serial split panes are disabled because a serial device is normally an exclusive writer.

## Port Forwards

Use the forwarding UI to create and manage local, remote, and dynamic forwards.

Forward types:

- Local: a local port connects through SSH to a remote target.
- Remote: a remote port connects back to a local target.
- Dynamic: a SOCKS-style tunnel.

Attach forwards to the owning connection so their lifecycle is clear. Enable auto-start only for forwards that should start whenever the connection opens. When testing a new forward, confirm both the forwarding row and the owning connection are healthy.

## Validation And Export

Use the app to inspect visible connection and forward state. Use the CLI companion for CI, reviewable exports, or support workflows:

```sh
oxideterm connections validate --strict
oxideterm connections export --format raw-safe --json
oxideterm forwards validate --json
```

`raw-safe` output is intended for review and automation without credential values.
