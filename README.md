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

[release]: https://github.com/josiahbull/ff-router/releases/latest

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
