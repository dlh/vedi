# Development

Requires Rust and make. The other tools, at the versions CI uses, come from
[mise](https://mise.jdx.dev): `.config/mise.toml` lists them and
`.config/mise.lock` pins their checksums. `mise install` installs them, and
`mise activate` in the shell puts them on PATH. vhs also needs ttyd and ffmpeg,
which mise cannot install on macOS: `brew install ttyd ffmpeg`. `make aur` and
the secrets script need [gh](https://cli.github.com), at any version.

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
before committing. `licenses` runs
[cargo-about](https://github.com/EmbarkStudios/cargo-about);
`.config/about.toml` lists the licenses a dependency may carry, each compatible
with GPL-3.0-only. Adding one is a decision, not a fix for a failing check.
Behaviors are specified and tested in `specs/`; see `specs/README.md`.
`bin/spec-step FILE` steps through one, showing the screen after each section.

The man page, `vedi/man/vedi.1`, is hand-written mdoc. A change to a flag, a
default key, a setting or a `:` command edits it in the same commit, and its
`.Dd` date. `make test` checks that it names every flag of the usage text, every
key of `defaults.conf`, every config verb and every `:` command, and, where
mandoc is on PATH, that `mandoc -T lint` passes: macOS ships mandoc, and the
linux CI container installs it. `man vedi/man/vedi.1` shows it.

Prose in `.md` files wraps at 80 columns; tables and code blocks do not.
`.git-blame-ignore-revs` lists the commits that only rewrapped, for
`git config blame.ignoreRevsFile .git-blame-ignore-revs`.

`bench-pagers` runs each pager on a pty over a million generated lines and
reports the median time to the first screen from a file and from stdin, to the
end and back, and for a failed search on each, with peak RSS. To the end is the
rest of the index for vedi and a seek for less. The pty answers a device
attributes query, as a terminal would, and the harness reads the screen, since a
pager may redraw only the cells that changed. Each pager runs with its defaults:
the config home is a directory without one.

`BENCHFLAGS='--lines N --runs R'` changes the input and the repetitions. The
file is read through before each pager, so none reads it from disk. vim runs
with `--plugins DIR`, a directory holding a checkout of
[vim-plugin-AnsiEsc](https://github.com/powerman/vim-plugin-AnsiEsc) under that
name, which colors the escapes; without it, vim is left out.

The table in `docs/comparison.md` comes from a run with the defaults and the
plugin. neovim, with [baleia.nvim](https://github.com/m00qek/baleia.nvim), was
benchmarked until vedi 1.9.0 and then dropped.

`media` runs `docs/media/demo.tape`, `hint.tape` and `filter.tape` with
[vhs](https://github.com/charmbracelet/vhs): the first two over the checked-in
`docs/media/sample.txt`, lorem ipsum with SGR colors and an OSC 8 link, the
third over `vedi/src/app/status.rs`, read through bat as the `open_cmd`. vedi
runs with its defaults otherwise: the config home is `docs/media`, which holds
no config. It then runs `bin/theme-previews`, which fills in
`docs/media/themes/theme.tape` for each theme in `vedi/themes/` and writes a
screenshot beside it; a new theme needs its terminal colors added to the script.
Run it after a change to the drawing or a theme, and commit what it rewrites.

`make docs/media/hint.gif` records one gif, and only when its tape or input is
newer; `-B` forces it, as after a change to vedi alone. `make media-gifs`
records the gifs, `make media-themes` the theme screenshots.

Commit messages follow
[Conventional Commits](https://www.conventionalcommits.org/). `CHANGELOG.md`
is built from their subjects at release time: a breaking change, `feat:`,
`fix:` and `perf:` each get a section; `refactor:`, `test:`, `docs:`,
`build:`, `ci:` and `chore:` are left out, since they change nothing a user
sees. A subject is the changelog line, linked to its commit, so it says what
vedi does now, for the user; keys, flags, settings and commands go in
backticks: "`&` starts with the search pattern".

## Keys

vedi asks a terminal that speaks the kitty keyboard protocol for its first flag
only, disambiguate escape codes. Under it a chord that produces text arrives as
that text: Shift+Space is a space, and Option+key on macOS is the composed
character unless the terminal is set to send Alt. Such a chord cannot be a
default binding: a default works in terminals as shipped, and a setting the user
applies belongs in `docs/terminals.md`. Shift+Space as page up was tried and
dropped for this. Terminal.app is the exception: it lacks mappings for many
modified keys, such as ⇧↑ and ⌥↑, so those are bound and its Send Text fixes
are listed.

The protocol's report-all-keys flag would deliver the key, but the composed text
then comes only in the associated-text field, which crossterm 0.29 drops;
prompts would lose dead-key and IME input.
[crossterm#965](https://github.com/crossterm-rs/crossterm/issues/965) tracks it.
`kitty +kitten show_key -m kitty` shows what kitty sends with every flag on.

A legacy terminal sends Ctrl+Backspace as Ctrl+h, which vedi takes for
Backspace; only under the protocol does it delete a word at a prompt.

## Releases

`make next-version` prints the next tag: a breaking change (`type!:` or a
`BREAKING CHANGE:` footer) bumps major, a `feat:` minor, a `fix:` or `perf:`
patch. It fails when the commits since the last tag hold none of those.
`make release-tag` prepends that tag's entry to `CHANGELOG.md`, from the commit
subjects, commits it, tags HEAD and prints the push that releases it:

    make release-tag
    git push origin main v1.2.3

To edit the entry first, `make changelog` writes it without committing
(`bin/changelog` prints it instead); commit it, and `release-tag` only tags.
`release-tag` stops when `make audit` fails.

CI builds and tests linux amd64 and arm64 and darwin arm64, each on a runner of
its kind (`.github/workflows/build.yml`). The linux builds run in an Ubuntu
22.04 container, so those binaries need glibc 2.34 or newer. A tag runs the same
builds, then goreleaser packages them: its Rust builder calls
`bin/cargo-prebuilt`, which puts each binary where cargo would have. It
publishes a GitHub release with archives, a source archive, `checksums.txt`, its
signature and the tag's `CHANGELOG.md` entry as its notes. Each archive holds:

- `CHANGELOG.md`
- `LICENSE-THIRD-PARTY`, which `make licenses` writes before the build
- the completion scripts of `vedi/completions/`, in `completions/`
- the man page `vedi/man/vedi.1`, in `man/`
- the themes of `vedi/themes/`, in `themes/`
- the Markdown of `docs/`, in `docs/`

The source archive holds the tree and `LICENSE-THIRD-PARTY`.

cosign signs `checksums.txt` as the workflow, without a key, before anything is
published. The `attest` job then takes `checksums.txt` from the release, checks
that signature and attests each file it lists. It runs even when goreleaser
fails after publishing; if it fails itself, re-run that job alone. Releases are
immutable: a published one cannot be fixed, only followed by the next version.
`SECURITY.md` shows how to verify a download.

goreleaser then pushes a cask to
[dlh/homebrew-tap](https://github.com/dlh/homebrew-tap) and two packages to the
AUR:
[vedi](https://aur.archlinux.org/packages/vedi) builds the source archive, and
[vedi-bin](https://aur.archlinux.org/packages/vedi-bin) installs the linux
archives. Both install `LICENSE-THIRD-PARTY` under `/usr/share/licenses`; the
GPL is in Arch's `licenses` package. They install the themes in
`/usr/share/vedi/themes`, and the cask in `$(brew --prefix)/share/vedi/themes`.

`vedi -v` prints the tag, passed as `VEDI_VERSION`; builds from a checkout print
`git describe`, and others `unknown`.

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
