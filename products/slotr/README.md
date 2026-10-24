# slotr

slotr is a queue for heavy commands on one Linux machine, such as dev
servers and test stacks. `slotr run` waits for a free slot and enough free
memory, then runs your command in its own
[systemd user service](docs/install.md#requirements). When memory runs low,
slotr can stop the lowest-priority evictable command, newest first within a
priority level, and it can reclaim a run past its lease when another
campaign waits. This lowers the risk of running out
of memory but does not remove it.

## Quick install

```sh
curl -fsSL -o slotr-install.sh \
  https://github.com/olafurns7/herdr-kit/releases/download/slotr/v0.3.2/install.sh &&
  printf '%s  %s\n' dd11030f1a59b4bf828b0b5c388ce4793a02e6b6cc7a3383c1f0ba1f8636e919 slotr-install.sh | sha256sum -c - &&
  SLOTR_VERSION=slotr/v0.3.2 sh slotr-install.sh
```

Downloads the Linux x86_64 release, verifies the installer and archive
checksums, and installs to `~/.local/bin`. For the latest stable release or a
different pin with `SLOTR_VERSION`, see [install](docs/install.md#versions).

## Quick agent setup

After downloading and verifying the installer above:

```sh
SLOTR_VERSION=slotr/v0.3.2 sh slotr-install.sh --config
```

This also creates the config from the example when none exists, then checks it.

## Documentation

- [Install](docs/install.md)
- [Agent setup](docs/agent-setup.md)
- [Usage and campaign priority commands](docs/usage.md#commands)
- [Cooperative flock guard](docs/flock.md)
- [Configuration](docs/configuration.md)
- [How it works](docs/how-it-works.md)
- [Troubleshooting](docs/troubleshooting.md)
- [Validate and release](docs/release.md)

License: [MIT](LICENSE). [Third-party notices](THIRD-PARTY-NOTICES) ship in the release archive.

See [install](docs/install.md#versions) for version selection.
