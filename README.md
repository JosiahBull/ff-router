# firefox-link-router

A tiny macOS "default browser" that opens each link in the right Firefox
profile. Set it as your default browser; when you click a link it matches the
URL against globs in `~/.ff-router.toml` and re-launches Firefox with the
matching `--profile`.

## Install

```sh
./scripts/install.sh
```

This builds a size-optimised release binary (~480 KB), wraps it in
`Firefox Router.app`, installs it to `~/Applications`, and registers it with
Launch Services. Then set it as the default browser:

**System Settings → Desktop & Dock → Default web browser → Firefox Router**

| Script | Purpose |
| --- | --- |
| `scripts/build.sh` | Build the optimised release binary into `target/<triple>/release/` |
| `scripts/package.sh` | Build + assemble the signed `dist/Firefox Router.app` |
| `scripts/install.sh` | Package + install to `~/Applications` + register |
| `scripts/uninstall.sh` | Remove and deregister the installed app |

The build uses nightly `build-std` with `panic=immediate-abort` plus
`opt-level=z` + LTO + strip (see `Cargo.toml` and `scripts/build.sh`). It stays
a small static binary — no launch-time decompression — so startup is fast and
resident memory is minimal. The pinned nightly toolchain is in
`rust-toolchain.toml`.

## Configure

Copy the example and edit it:

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

Find your profile directory names under
`~/Library/Application Support/Firefox/Profiles/`. If Firefox has no config or
no rule matches and there's no `default`, links open in Firefox's own default
profile.

## Test without changing your default browser

```sh
cargo test                                    # routing logic
cargo run -- https://team.atlassian.net/x     # route one URL now
```
