# Connections and Forwards

## Saved Connections

Saved connections hold reusable SSH profile data: name, host, user, port, group, tags, color, authentication mode, and optional post-connect command.

List and inspect profiles:

```sh
oxideterm connections list
oxideterm connections show prod --json
oxideterm connections search prod
```

Create a profile with direct parameters:

```sh
oxideterm connections create \
  --name prod \
  --host example.internal \
  --user deploy \
  --port 22 \
  --group production \
  --auth agent \
  --dry-run
```

Repeat with `--yes` after reviewing the plan.

## Groups

Groups keep connection lists readable:

```sh
oxideterm connections groups
oxideterm connections group add production --yes
oxideterm connections group rename production prod --yes
```

Use groups for human navigation, not for storing environment secrets.

## Validation and Export

Run validation before imports, CI checks, or support reports:

```sh
oxideterm connections validate --strict
oxideterm connections export --format raw-safe --json
```

`raw-safe` output is intended for review and automation without credential values.

## Port Forwards

Forwards can be managed independently from `.oxide` bundles:

```sh
oxideterm forwards list
oxideterm forwards create \
  --type local \
  --bind-port 8080 \
  --target-host localhost \
  --target-port 80 \
  --connection prod \
  --dry-run
oxideterm forwards validate --json
```

Forward types:

- `local`: local port to remote target.
- `remote`: remote port to local target.
- `dynamic`: SOCKS-style dynamic forwarding.

Use `--auto-start` only when the forward should start whenever the owning connection opens.
