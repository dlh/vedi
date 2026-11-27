# Development

- [Tools](#tools)
- [Checks](#checks)
- [Specs](#specs)
- [Man page](#man-page)
- [Markdown](#markdown)
- [Pager benchmarks](#pager-benchmarks)
- [Media](#media)
- [Commits](#commits)
- [Default keys](#default-keys)
- [Releases](#releases)

## Tools

Requires Rust and make. The other tools, at the versions CI uses, come from
[mise](https://mise.jdx.dev):

- `.config/mise.toml` lists them and `.config/mise.lock` pins their checksums.
- `mise install` installs them, and `mise activate` in the shell puts them on
  PATH.
- vhs also needs ttyd and ffmpeg, which mise cannot install on macOS:
  `brew install ttyd ffmpeg`.
- `make aur` and the secrets script need [gh](https://cli.github.com), at any
  version.

## Checks

    make test           clippy, then cargo test --workspace
    make fmt            show what rustfmt would change (fails if any)
    make tidy           fail if Cargo.lock is out of date
    make licenses       write target/LICENSE-THIRD-PARTY; fail on a license .config/about.toml does not accept
    make audit          fail on a RustSec vulnerability in Cargo.lock
    make outdated       list dependency updates published at least two weeks ago
    make bench          criterion benchmarks
    make bench-compare  benchmarks at BENCH_GIT_BRANCH (main) as a baseline, then here against it, BENCH_COUNT (10) samples each
    make install        cargo install --path vedi --locked
    make bench-pagers   vedi, less, moor, ov and vim on a large file; needs them on PATH
    make release-check  validate .goreleaser.yaml (needs goreleaser and a git remote)
    make next-version   print the next tag, from the commits since the last
    make changelog      prepend the next release's entry to CHANGELOG.md, to edit before release-tag
    make release-notes  print the notes the next release carries: the first CHANGELOG.md entry
    make release-tag    commit the CHANGELOG.md entry and tag HEAD with the next version
    make aur            write the AUR packages to dist/aur/ (needs goreleaser and gh)
    make media          record the README gifs and theme screenshots

CI runs `fmt`, `test`, `tidy` and `licenses` on every push and PR; run them
before committing.

`licenses` runs [cargo-about](https://github.com/EmbarkStudios/cargo-about).
`.config/about.toml` lists the licenses a dependency may carry, each compatible
with GPL-3.0-only. Adding one is a decision, not a fix for a failing check.

## Specs

Behaviors are specified and tested in `specs/`; see `specs/README.md`.
`bin/spec-step FILE` steps through one, showing the screen after each section.

## Man page

The man page, `vedi/man/vedi.1`, is hand-written mdoc. A change to a flag, a
default key, a setting or a `:` command edits it in the same commit, and its
`.Dd` date. `man vedi/man/vedi.1` shows it.

`make test` checks that it names:

- every flag of the usage text
- every key of `defaults.conf`
- every config verb
- every `:` command

Where mandoc is on PATH, it also checks that `mandoc -T lint` passes. macOS
ships mandoc, and the linux CI container installs it.

## Markdown

Prose in `.md` files wraps at 80 columns; tables and code blocks do not.
`.git-blame-ignore-revs` lists the commits that only rewrapped, for
`git config blame.ignoreRevsFile .git-blame-ignore-revs`.

`make test` checks two docs against the code:

- The tables in `docs/keys.md` are the help screen's sections and rows.
- In `docs/configuration.md`, the Defaults block is `defaults.conf`, the vim and
  emacs blocks parse, and the Colors table names each element and its default.

## Pager benchmarks

`bench-pagers` runs each pager on a pty over a million generated lines. It
reports the median time for each of these, with peak RSS:

- the first screen, from a file and from stdin
- to the end and back; to the end is the rest of the index for vedi and a seek
  for less
- a failed search, on each input

How it runs:

- The pty answers a device attributes query, as a terminal would.
- The harness reads the screen, since a pager may redraw only the cells that
  changed.
- Each pager runs with its defaults: the config home is a directory without one.
- The file is read through before each pager, so none reads it from disk.
- `BENCHFLAGS='--lines N --runs R'` changes the input and the repetitions.
- vim runs with `--plugins DIR`, a directory holding a checkout of
  [vim-plugin-AnsiEsc](https://github.com/powerman/vim-plugin-AnsiEsc) under
  that name, which colors the escapes. Without it, vim is left out.

The table in `docs/comparison.md` comes from a run with the defaults and the
plugin.

## Media

`make media` records the gifs with
[vhs](https://github.com/charmbracelet/vhs), then the theme screenshots. Run it
after a change to the drawing or a theme, and commit what it rewrites.

| Tape in `docs/media/` | Input |
|---|---|
| `demo.tape`, `hint.tape` | The checked-in `docs/media/sample.txt`, lorem ipsum with SGR colors and an OSC 8 link |
| `filter.tape` | `vedi/src/app/status.rs`, read through bat as the `open_cmd` |

vedi runs with its defaults otherwise: the config home is `docs/media`, which
holds no config.

`bin/theme-previews` fills in `docs/media/themes/theme.tape` for each theme in
`vedi/themes/` and writes a screenshot beside it. A new theme needs its terminal
colors added to the script.

| Command | Records |
|---|---|
| `make media-gifs` | The gifs |
| `make media-themes` | The theme screenshots |
| `make docs/media/hint.gif` | One gif, and only when its tape or input is newer; `-B` forces it, as after a change to vedi alone |

## Commits

Commit messages follow
[Conventional Commits](https://www.conventionalcommits.org/). `CHANGELOG.md`
is built from their subjects at release time:

- A breaking change, `feat:`, `fix:` and `perf:` each get a section.
- `refactor:`, `test:`, `docs:`, `build:`, `ci:` and `chore:` are left out,
  since they change nothing a user sees.

A subject is the changelog line, linked to its commit, so it says what vedi does
now, for the user. Keys, flags, settings and commands go in backticks:
"`&` starts with the search pattern".

## Default keys

A default key works in terminals as shipped. A key that needs a terminal setting
is not bound, and the setting goes in `docs/terminals.md`.

Terminal.app is the exception. It lacks mappings for many modified keys, such as
⇧↑ and ⌥↑, so those are bound and its Send Text fixes are listed in
`docs/terminals.md`.

## Releases

### Tagging

`make next-version` prints the next tag. It fails when the commits since the
last tag hold none of these:

| Commit | Bumps |
|---|---|
| A breaking change: `type!:` or a `BREAKING CHANGE:` footer | major |
| `feat:` | minor |
| `fix:` or `perf:` | patch |

`make release-tag` prepends that tag's entry to `CHANGELOG.md`, from the commit
subjects, commits it, tags HEAD and prints the push that releases it:

    make release-tag
    git push origin main v1.2.3

To edit the entry first, `make changelog` writes it without committing
(`bin/changelog` prints it instead); commit it, and `release-tag` only tags.
`release-tag` stops when `make audit` fails.

### What a tag publishes

A pushed tag runs `.github/workflows/release.yml`, which publishes:

- A GitHub release: an archive for each platform, a source archive, a signed
  `checksums.txt` and the tag's `CHANGELOG.md` entry as its notes.
- A cask in [dlh/homebrew-tap](https://github.com/dlh/homebrew-tap).
- Two AUR packages: [vedi](https://aur.archlinux.org/packages/vedi) builds the
  source archive, and [vedi-bin](https://aur.archlinux.org/packages/vedi-bin)
  installs the linux archives.

`.goreleaser.yaml` lists what each archive and package holds.

Releases are immutable: a published one cannot be fixed, only followed by the
next version. If the `attest` job fails, re-run that job alone. `SECURITY.md`
shows how to verify a download.

`vedi -v` prints the tag, passed as `VEDI_VERSION`; builds from a checkout print
`git describe`, and others `unknown`.

### Secrets

Darwin binaries are signed with a Developer ID Application certificate and
notarized (bare binaries can't be stapled; Gatekeeper checks the ticket online).
A tag release fails when a secret below is absent; elsewhere goreleaser skips
the steps that need it. The secrets live in the GitHub `release` environment,
which only the release workflow on a pushed `v*` tag can use. The API key comes
from App Store Connect: Users and Access → Integrations → Team Keys, Developer
role.

    APPLE_CERTIFICATE_P12_BASE64   the certificate, base64
    APPLE_CERTIFICATE_PASSWORD     its password
    APPLE_API_ISSUER_ID            API key issuer ID
    APPLE_API_KEY_ID               API key ID
    APPLE_API_KEY_P8_BASE64        API key .p8, base64
    HOMEBREW_TAP_TOKEN             GitHub token that pushes to the tap
    AUR_KEY                        SSH private key that pushes to the AUR

The tap token is a fine-grained personal access token: Settings → Developer
settings → Fine-grained tokens, repository `dlh/homebrew-tap` only, Contents
read and write. The AUR key's public half goes on the AUR account: My Account →
SSH Public Key. The first push creates the package.

Export the certificate from Keychain Access (My Certificates → Developer ID
Application → File → Export Items… → .p12). Encode both files without line
breaks; goreleaser rejects wrapped base64:

    base64 -i Certificates.p12 | tr -d ' \n' | pbcopy
    base64 -i AuthKey_XXXXXXXXXX.p8 | tr -d ' \n' | pbcopy

To load them from a 1Password item whose fields carry the same names:

    bin/sync_secrets_from_1password.sh "op://Private/GitHub vedi Secrets"

### Trying a release locally

To try a release locally without tagging:

    goreleaser build --snapshot --clean --single-target

Output lands in `dist/`, which is ignored. Only this platform's binary builds
here; the whole pipeline, short of notarizing, publishing and attesting, runs
from the Actions tab: release, Run workflow.

`make aur` writes the AUR packages to `dist/aur/vedi/` and `dist/aur/vedi-bin/`:
each `PKGBUILD`, its `.SRCINFO` and its sources, named as the `PKGBUILD` looks
for them, so makepkg builds without a release. vedi-bin takes the last
release's binaries, since only CI builds the linux ones. Copy a directory to an
Arch machine and run `makepkg -si` in it.
