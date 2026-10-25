# Connections and Forwards

Use the Sessions, Connection Pool, Connection Monitor, Host Tools, graphics/VNC, and forwarding surfaces for normal SSH work. The CLI companion is only for headless validation, export, and repeatable setup.

## Saved Connections

Saved connections hold reusable SSH profile data: name, host, user, port, group, tags, color, authentication mode, and optional post-connect command.

Create and edit saved connections from the connection manager or Sessions view. For a new host, fill in the profile, choose an authentication mode, save it, then open the connection from Sessions. If the connection fails, edit the same saved profile instead of creating duplicate entries with similar labels.

Use groups, colors, and tags for navigation. Do not put passwords, tokens, or environment secrets in names, groups, tags, notes, or post-connect labels.

## Kerberos authentication

Use Kerberos when your organization provides a current system sign-in or ticket and the SSH server accepts Kerberos authentication.

1. Create or edit an SSH connection and enable **Prefer Kerberos** under authentication.
2. Check the credential-availability message. OxideTerm uses credentials from the current operating-system session; obtain or renew them through your organization's sign-in procedure if none are available.
3. Leave **Kerberos server identity (optional)** empty to use `host/<SSH host>`. When connecting through an alias or load balancer, enter the service identity provided by your administrator, such as `host/server.example.com`.
4. Choose the **Fallback authentication** method and configure its password, key, or agent as needed.
5. Save and connect. If authentication fails, check the connection progress, ticket availability, server identity, and the server's Kerberos support.

**Delegate Kerberos credentials to this server** allows the remote host to use delegated credentials to access other Kerberos services as you. Enable it only when that workflow is needed on a trusted server. Configure each jump host's authentication separately.

## Automatic verification codes (TOTP)

1. Open **Settings → Credentials → TOTP credentials**, then choose **Add credential**.
2. Give it a recognizable name and enter the Base32 secret or `otpauth://` link from the service. This is the setup secret, not the current six-digit code.
3. Check the algorithm, 6- or 8-digit length, and period of 1–300 seconds against the service. An imported link supplies its own parameters; save it before editing those parameters separately.
4. Review the prompt-matching regular expression, leave the credential enabled, and save.
5. Edit the SSH connection and select the credential in **TOTP credential**. Select it separately on each jump host that needs it, then reconnect.

OxideTerm generates a code when a matching SSH authentication prompt appears. Both hidden and visible-input verification prompts are supported, including matching JumpServer prompts. Other prompts still require their own answers. If the code is not filled in, check that the credential is enabled, bound to the correct host, and matches the actual prompt. If it is rejected, check the device clock and the service's code parameters. Existing custom prompt expressions are preserved when updating OxideTerm.

When editing a credential, leave the secret empty to keep its saved value. Remove its bindings from connections and jump hosts before deleting it. See [troubleshooting](troubleshooting.md#connection-and-transfer-checks) for authentication checks.

## Mosh connections

Mosh is useful for a terminal that must continue across brief network changes. First install and enable **Mosh** in **Plugin Manager → Marketplace** on this device. The plugin supplies the local transport; the remote host still needs `mosh-server` installed and a reachable UDP port.

1. Open New Connection and choose **Mosh**. Enter the host, username, SSH port, and authentication used to start the remote session.
2. In **Mosh Settings**, leave **mosh-server executable** as `mosh-server`, or set its remote path if it is not on the server's command search path.
3. Leave **UDP port or range** empty for automatic selection, or enter an allowed port such as `60001` or a range such as `60000:61000`. Allow the selected UDP port or range through the server firewall and any network address translation.
4. Use **UDP host override** if the UDP endpoint must use a different public host or address. Set the remote UTF-8 locale if the server requires one. Local prediction defaults to **Adaptive**.
5. Save the profile if you want to reuse it, then open the connection and confirm that the terminal accepts input.

SSH and any jump hosts are used to start the remote session. The subsequent UDP connection must be directly reachable from this device; an SSH jump route does not carry it. Each Mosh connection opens one UTF-8 terminal. Open a separate SSH connection for SFTP, IDE, or port forwarding. If startup succeeds but the terminal cannot communicate, check the UDP host, selected port, firewall, and address-family settings.

## Importing Connections

### OpenSSH configuration

**Settings → Connections → Import from SSH Config** shows the configuration file being scanned. Host discovery, alias lookup, and automatic synchronization use this same path. In portable mode, `<data directory>/.ssh/config` takes priority when it exists; otherwise OxideTerm reads the current user's `~/.ssh/config`. An empty portable config keeps the host list empty. Portable private-key discovery, imported keys, and `known_hosts` continue to use the portable data directory.

### Other clients

Open Settings, go to Connections, and use **Import from Other Clients**. Select a source, choose its file or folder, review the preview, then import the selected connections. Existing names can be skipped or renamed, and an optional target group can override source groups.

Supported sources and inputs:

| Source | Input |
|--------|-------|
| SecureCRT | Session `.ini` files, a Sessions folder, or a SecureCRT `.xml` export |
| Xshell | `.xsh` files, a Sessions folder, or an `.xts` archive |
| Termius | Exported JSON |
| MobaXterm | `.mxtsessions` export |
| WindTerm | `user.sessions` JSON |
| Electerm | Bookmarks JSON containing `bookmarkGroups` and `bookmarks`, or a legacy bookmark array |
| FinalShell | The `conn` folder, or the FinalShell data folder that contains `conn` |

OxideTerm imports safe connection metadata such as names, groups, hosts, ports, usernames, and supported key-file paths. Passwords, passphrases, embedded private keys, certificates, proxy credentials, and other secret values are not imported. Unsupported proxy, jump-host, and forwarding settings are shown as preview warnings. Configure authentication and review those advanced settings in OxideTerm after the metadata import.

## Connection Runtime

Saved profiles and live runtime nodes are different things:

- Saved profile: the host and connection settings OxideTerm should use.
- SSH node: the live or reconnecting runtime state for a host.
- Terminal session: a visible shell attached to an SSH node.
- SFTP session: a file browsing or transfer surface attached to an SSH node.
- Host Tools snapshot: a resource view sampled through the owning SSH node.
- Graphics/VNC session: a viewer backed by a saved RDP/VNC profile or a node-owned graphics runtime.

Use Connection Pool and Connection Monitor when a terminal looks stuck, SFTP cannot read a directory, Host Tools look stale, a graphics/VNC viewer disconnects, or reconnect behavior is unclear. Reconnect the runtime from the app state; do not delete and recreate the saved profile just to reconnect.

## Connecting

Typical flow:

1. Open Sessions.
2. Select a saved connection or create one.
3. Open the connection.
4. Wait for the SSH node and terminal tab to become live.
5. If needed, open SFTP, IDE, Host Tools, or forwarding from the same connected node, or open a saved RDP/VNC profile.

For unstable hosts, keep Connection Monitor open while testing. It shows whether a node is connected, connecting, stale, or unavailable.

## Host Tools

Host Tools are node-level views for inspecting the remote environment. Use them for processes, Docker, services, tmux, packages, logs, ports, filesystems, scheduled tasks, and metrics.

Resource snapshots can become stale independently of the terminal tab. Refresh the Host Tools page or reconnect the owning node before retrying actions. Host Tools actions should remain explicit and reviewable; do not use them as hidden background cleanup.

## Graphics And VNC

For RDP display-layout support, use host 2.2.5 or later with RDP plugin 0.2.0 or later. Single-monitor mode can follow the viewer size or use a fixed resolution. All-monitor mode follows the active physical displays; resizing the window changes local fitting rather than overwriting the remote topology. Display changes and reconnect refresh the layout.

For RDP or VNC, open **Plugin Manager → Marketplace**, install and enable the corresponding **RDP** or **VNC** plugin, then choose that protocol in **New Connection**. Saved profiles remain in Session Manager; each device needs its own plugin installation, including when the profile arrives through cloud sync. Missing or disabled plugins are shown in the connection form.

Graphics/VNC sessions are visual app surfaces backed either by a saved RDP/VNC profile or by a connected node's graphics runtime. They are useful when a remote workflow needs a desktop or graphical application rather than terminal output.

The viewer state is separate from terminal scrollback. If a profile-backed viewer disconnects, reconnect its provider/helper; if a node-launched viewer disconnects, reconnect or restart it from the node context. Do not duplicate a saved SSH profile just to repair a viewer.

## Telnet Line Endings

Telnet exposes separate transmit and receive line-ending controls in the connection form and terminal control bar. Use transmit settings when Enter does not submit correctly, and receive settings when output lines are laid out incorrectly; these are separate from Backspace key mapping. Changes apply to the live terminal and are saved back to its associated Telnet profile when it has one.

Telnet does not encrypt traffic. Use it only on an appropriate trusted network or protected route; choose SSH where encrypted remote login is available.

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

## Privilege Credentials

In Settings → Credentials, use **SSH common** to save sudo, su, or custom-prompt credentials for your SSH sessions. Connection-specific matches take priority; local terminals have a separate scope.

SSH-common credentials can also define **host patterns** such as `hktkosl*` or `i-*`. The field starts with `*`, which matches every host. Matching supports `*` and `?`, is case-insensitive, and uses the active terminal's SSH host. The helper first keeps only enabled credentials that match the current prompt, then applies this order: connection-specific credential, then a host-matched SSH-common credential, then an SSH-common credential whose patterns are empty or only `*`. A disabled or non-matching connection-specific credential does not hide a matching shared credential.

Enable **Fill automatically and press Enter** to send a matching password without pressing Enter first. It applies only when exactly one credential matches. For SSH common, use a specific host pattern when you do not want the automation to apply to every host; the default `*` matches all hosts.

**Prompt type** and **Response mode** are independent. Prompt type tells the helper whether it is answering a sudo prompt, an su prompt, or a custom prompt fragment. Response mode selects **Password only**, **Password, then TOTP prompt**, or **Password + TOTP in one response**. Separate mode uses **Second-stage prompt patterns** plus an optional **Second-stage user hint** to identify the verification-code prompt; combined mode sends the password and code together and does not use the second-stage fields. Both TOTP modes require an existing verification-code credential for the seed. After the password is filled, the helper sends one code automatically when auto-fill is enabled, otherwise it shows an inline hint for Enter. A repeated second-factor prompt falls back to manual input.

When a password prompt has one matching credential, press Enter at the inline hint to fill it. With several matches, Enter opens a chooser showing each credential's name, type, and source. Use arrow keys and Enter, click an entry, or press Esc to cancel. Starting manual input or changing the terminal closes the chooser.

Full connection sync includes the common scope when sensitive-credential sync is enabled; selecting only some connections leaves that shared scope unchanged. An encrypted connection export with passwords includes scoped copies of common credentials whose host patterns match the exported connection, plus only the verification-code credentials those copies reference. Importing that package attaches the copies to the imported connections without creating a global default.

## Port Forwards

Use the forwarding UI to create and manage local, remote, and dynamic forwards.

Forward types:

- Local: a local port connects through SSH to a remote target.
- Remote: a remote port connects back to a local target.
- Dynamic: a SOCKS-style tunnel.

Attach forwards to the owning connection so their lifecycle is clear. Enable auto-start only for forwards that should start whenever the connection opens. When testing a new forward, confirm both the forwarding row and the owning connection are healthy.

## Standalone SSH Tunnels

Use a standalone tunnel when you need a forwarded port without opening a terminal.

1. Open **New Connection → Advanced Connections → SSH Tunnel**.
2. Enter a name and optional group, then select the saved SSH host that will carry the tunnel.
3. Choose **Local**, **Remote**, or **Dynamic (SOCKS5)**. Set the bind address and port; local and remote forwards also need a destination host and port.
4. Choose **Save** for later use, or **Save and Start** to connect now.
5. Enable **Connect on application startup** only if this tunnel should connect whenever OxideTerm starts. Authentication may still require your input.

Manage saved tunnels from **Session Manager** and inspect running rules in **Port Forwarding**. A tunnel retains its own SSH connection usage, so closing an unrelated terminal does not stop it. Stop the tunnel explicitly when it is no longer needed. Exiting OxideTerm stops its tunnels; they do not run as an independent background service.

Tunnel profiles participate in cloud sync and encrypted `.oxide` import/export. Running connections are not synchronized. After moving a profile to another device, check the associated SSH host, authentication, and local bind port before starting it.

## Validation And Export

Use the app to inspect visible connection and forward state. Use the CLI companion for CI, reviewable exports, or support workflows:

```sh
oxideterm connections validate --strict
oxideterm connections export --format raw-safe --json
oxideterm forwards validate --json
```

`raw-safe` output is intended for review and automation without credential values.
