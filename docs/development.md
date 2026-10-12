# Development

Requires Rust and make.

## Checks

    make test           clippy, then cargo test --workspace
    make fmt            show what rustfmt would change (fails if any)
    make tidy           fail if Cargo.lock is out of date
    make bench          criterion benchmarks
    make bench-compare  benchmarks at BENCH_GIT_BRANCH (main) as a baseline, then here against it, BENCH_COUNT (10) samples each
    make install        cargo install --path vedi --locked
    make bench-pagers   vedi, less, moor, ov and vim on a large file; needs them on PATH
    make release-check  validate .goreleaser.yaml (needs goreleaser and a git remote)
    make next-version   print the next tag, from the commits since the last
    make release-notes  print the changelog the next release would carry (needs goreleaser)
    make release-tag    tag HEAD with the next version
    make media          record the README gif; needs vhs on PATH

CI runs `fmt`, `test` and `tidy` on every push and PR; run them before
committing. Behaviors are specified and tested in `specs/`; see
`specs/README.md`. `bin/spec-step FILE` steps through one, showing the screen
after each section.

Prose in `.md` files wraps at 80 columns; tables and code blocks do not.
`.git-blame-ignore-revs` lists the commits that only rewrapped, for
`git config blame.ignoreRevsFile .git-blame-ignore-revs`.

`bench-pagers` runs each pager on a pty over a million generated lines and
reports the median time to the first screen from a file and from stdin, to the
end and back, and for a failed search on each, with peak RSS. To the end is the
rest of the index for vedi and a seek for less. The pty answers a device
attributes query, as a terminal would, and the harness reads the screen, since a
pager may redraw only the cells that changed. Each pager runs with its defaults:
the config home is a directory without one. `BENCHFLAGS='--lines N --runs R'`
changes the input and the repetitions. vim runs with `--plugins DIR`, a
directory holding a checkout of
[vim-plugin-AnsiEsc](https://github.com/powerman/vim-plugin-AnsiEsc) under that
name, which colors the escapes, and is left out without it. The file is read
through before each pager, so none reads it from disk. The table in
`docs/comparison.md` comes from a run with the defaults and the plugin. neovim,
with [baleia.nvim](https://github.com/m00qek/baleia.nvim), was benchmarked until
vedi 1.9.0 and then dropped.

`media` runs `docs/media/demo.tape` with
[vhs](https://github.com/charmbracelet/vhs) (`brew install vhs`, which brings
ttyd and ffmpeg) over the checked-in `docs/media/sample.txt`, lorem ipsum with
SGR colors and an OSC 8 link. vedi runs with its defaults: the config home is
`docs/media`, which holds no config. Run it after a change to the drawing, and
commit the gif it rewrites.

Commit messages follow
[Conventional Commits](https://www.conventionalcommits.org/). The release
changelog is built from them: `feat:`, `fix:` and `perf:` each get a section;
`refactor:`, `test:`, `docs:`, `build:`, `ci:` and `chore:` are left out, since
they change nothing a user sees.

## Keys

vedi asks a terminal that speaks the kitty keyboard protocol for its first flag
only, disambiguate escape codes. Under it a chord that produces text arrives as
that text: Shift+Space is a space, and Option+key on macOS is the composed
character unless the terminal is set to send Alt. Such a chord cannot be a
default binding: a default works in terminals as shipped, and a setting the user
applies belongs in `docs/terminals.md`. Shift+Space as page up was tried and
dropped for this.

The protocol's report-all-keys flag would deliver the key, but the composed text
then comes only in the associated-text field, which crossterm 0.29 drops;
prompts would lose dead-key and IME input.
[crossterm#965](https://github.com/crossterm-rs/crossterm/issues/965) tracks it.
`kitty +kitten show_key -m kitty` shows what kitty sends with every flag on.

## Releases

`make next-version` prints the next tag: a breaking change (`type!:` or a
`BREAKING CHANGE:` footer) bumps major, a `feat:` minor, a `fix:` or `perf:`
patch. It fails when the commits since the last tag hold none of those.
`make release-notes` prints the changelog that tag would carry.
`make release-tag` tags HEAD with it and prints the push that releases it:

    make release-tag
    git push origin v1.2.3

CI builds linux amd64 and arm64 and darwin arm64 with goreleaser's Rust builder
and cargo-zigbuild, on a macOS runner since the darwin targets link CoreServices
and need the macOS SDK. It publishes a GitHub release with archives,
`checksums.txt` and a changelog from the commit messages. Each archive holds the
completion scripts of `vedi/completions/` in `completions/`. It then pushes a
cask to [dlh/homebrew-tap](https://github.com/dlh/homebrew-tap), skipped without
`HOMEBREW_TAP_TOKEN`. `vedi -v` prints the tag, passed as `VEDI_VERSION`; builds
from a checkout print `git describe`.

Darwin binaries are signed with a Developer ID Application certificate and
notarized (bare binaries can't be stapled; Gatekeeper checks the ticket online).
Both steps are skipped when the secrets are absent. The secrets live in the
GitHub `release` environment, which only the release workflow on a `v*` tag can
use. The API key comes from App Store Connect: Users and Access → Integrations →
Team Keys, Developer role.

    APPLE_CERTIFICATE_P12_BASE64   the certificate, base64
    APPLE_CERTIFICATE_PASSWORD     its password
    APPLE_API_ISSUER_ID            API key issuer ID
    APPLE_API_KEY_ID               API key ID
    APPLE_API_KEY_P8_BASE64        API key .p8, base64
    HOMEBREW_TAP_TOKEN             GitHub token that pushes to the tap

The tap token is a fine-grained personal access token: Settings → Developer
settings → Fine-grained tokens, repository `dlh/homebrew-tap` only, Contents
read and write.

Export the certificate from Keychain Access (My Certificates → Developer ID
Application → File → Export Items… → .p12). Encode both files without line
breaks; goreleaser rejects wrapped base64:

    base64 -i Certificates.p12 | tr -d ' \n' | pbcopy
    base64 -i AuthKey_XXXXXXXXXX.p8 | tr -d ' \n' | pbcopy

To load them from a 1Password item whose fields carry the same names:

    bin/sync_secrets_from_1password.sh "op://Private/GitHub vedi Secrets"

To try a release locally without tagging:

    brew install goreleaser zig
    cargo install --locked cargo-zigbuild
    goreleaser release --snapshot --clean --skip=notarize

Output lands in `dist/`, which is ignored.
