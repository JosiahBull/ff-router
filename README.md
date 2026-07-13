# firefox-link-router

A tiny macOS "default browser" that opens each link in the right Firefox
profile. Set it as your default browser; when you click a link it matches the
URL against globs in `~/.ff-router.toml` and re-launches Firefox with the
matching `--profile`.

## Install

```sh
bash -c "$(curl -fsSL https://raw.githubusercontent.com/josiahbull/ff-router/main/scripts/install.sh)"
```

Building from a local checkout instead:

```sh
./scripts/dev-install.sh
```

### Scripted / non-interactive install

For dotfiles bootstraps and other unattended setups, skip the TUI with
`--non-interactive`. The installer expects the referenced Firefox profiles to
already exist and reuses `~/.ff-router.toml` (or writes one from `--config`),
then downloads the release binary, assembles the app, registers it, and
installs the login item:

```sh
# Reuse an existing ~/.ff-router.toml, then install the app:
ff-router-installer --non-interactive

# Or supply the config and skip the default-browser prompt (e.g. in CI):
ff-router-installer --non-interactive --config ./ff-router.toml --no-set-default
```

Everything else is identical to the interactive install; only the profile
discovery and step-by-step confirmation are skipped. Run
`ff-router-installer --help` for the full flag list.

## Configure

The installer writes `~/.ff-router.toml` for you. To create or edit it by hand
instead, copy the example:

```sh
cp .ff-router.toml.example ~/.ff-router.toml
```

```toml
debug   = false               # true → log each open to ~/.ff-router.log
default = "personal"          # profile used when nothing matches

[profiles]                    # label -> Firefox profile directory (or abs path)
work     = "qtIifLeX.Profile 1"
personal = "dhutbqgo.default-release"

[[rule]]                      # first matching rule wins
profile = "work"
globs = ["*://*.atlassian.net/*", "*://github.com/partly*"]

[[rule]]                      # route by the app that opened the link
profile = "work"              # globs + source both present → both must match
source = ["com.tinyspeck.*", "Slack"]   # globs matched vs bundle id AND app name
```

A rule may match on the URL (`globs`), the opening application (`source`), or
both (in which case both must match). `source` globs are tested against the
opener's bundle id *and* its localized name, so `"Slack"` and
`"com.tinyspeck.*"` both work. Detection is best-effort: links opened without a
sender — a terminal `open`, Spotlight, some sandboxed apps — carry no opener, so
`source` rules simply don't match them and routing falls through to `globs`/`default`.

### Shared base + local overrides (`extends`)

Point `extends` at one or more base configs to merge them under this one. This
keeps shared defaults in a single tracked place (e.g. a dotfiles repo) while
`~/.ff-router.toml` holds per-machine tweaks:

```toml
# ~/.ff-router.toml — local, per-machine
extends = "~/.dotfiles/.ff-router.toml"   # or an array of paths

# machine-specific overrides:
[[rule]]
profile = "work"
globs = ["*://*.internal.acme.corp/*"]
```

Merge rules: this file wins over its bases; `[profiles]` tables merge key by
key; and the ordered `[[rule]]` list is concatenated with **this file's rules
first**, then each base in turn (so a local rule beats a shared one on an
overlapping URL, and the shared rules remain as the fallback). A leading `~/`
is expanded and a relative path resolves against the including file's
directory; a missing base is warned about and skipped.

### Debugging

Set `debug = true` to record why each link went where it did. The router
appends one line per open to `~/.ff-router.log` — the URL, the app that opened
it, and the rule that matched:

```
2026-07-08T01:14:08Z url=https://foo.slack.com/messages opener=Slack (com.tinyspeck.slackmacgap) matched rule #0 -> profile "work" (…/Profiles/qtIifLeX.Profile 1)
```

`opener=unknown` means the OS attached no sender (e.g. a terminal `open`). The
file grows unbounded, so turn `debug` back off when you're done (`tail -f
~/.ff-router.log` to watch it live).

## Uninstall

```sh
./scripts/uninstall.sh
```
