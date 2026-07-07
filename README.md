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
default = "personal"          # profile used when nothing matches

[profiles]                    # label -> Firefox profile directory (or abs path)
work     = "qtIifLeX.Profile 1"
personal = "dhutbqgo.default-release"

[[rule]]                      # first matching rule wins
profile = "work"
globs = ["*://*.atlassian.net/*", "*://github.com/partly*"]
```

## Uninstall

```sh
./scripts/uninstall.sh
```
