# Development

Requires Go and make.

## Checks

    make test           vet, staticcheck, go fix -diff, go test ./...
    make fmt            list unformatted files (fails if any)
    make tidy           show what go mod tidy would change (fails if any)
    make bench          benchmarks
    make bench-compare  benchmarks at BENCH_GIT_BRANCH (main) vs here, BENCH_COUNT (10) runs each
    make bench-pagers   vedi, less, moor, ov, vim and neovim on a large file; needs them on PATH
    make release-check  validate .goreleaser.yaml (needs a git remote)
    make next-version   print the next tag, from the commits since the last
    make release-notes  print the changelog the next release would carry
    make release-tag    tag HEAD with the next version
    make media          record the README gif; needs vhs on PATH

CI runs `fmt`, `test` and `tidy` on every push and PR;
run them before committing. Behaviors are specified and tested in
`specs/`; see `specs/README.md`. `bin/spec-step FILE` steps through
one, showing the screen after each section.

`bench-pagers` runs each pager on a pty over a million generated
lines and reports the median time to the first screen from a file and
from stdin, to the end and back, and for a failed search on each, with
peak RSS. To the end is the rest of the index for vedi and a seek for
less. The pty answers a device attributes query, as a terminal would,
and the harness reads the screen, since a pager may redraw only the
cells that changed. `BENCHFLAGS='-lines N -runs R'` changes the input
and the repetitions. The editors run with `-plugins DIR`, a directory
holding checkouts of
[vim-plugin-AnsiEsc](https://github.com/powerman/vim-plugin-AnsiEsc)
and [baleia.nvim](https://github.com/m00qek/baleia.nvim) under those
names, which color the escapes for vim and neovim, and are left out
without it. The pty also answers neovim's status and background
color queries, and the file is read through before each pager, since
neovim's run leaves it out of the page cache. The table in
`docs/comparison.md` comes from a run with the defaults and the
plugins.

`media` runs `docs/media/demo.tape` with [vhs](https://github.com/charmbracelet/vhs)
(`brew install vhs`, which brings ttyd and ffmpeg) over the checked-in
`docs/media/sample.txt`, lorem ipsum with SGR colors and an OSC 8 link. Run it after a change
to the drawing, and commit the gif it rewrites.

Commit messages follow [Conventional
Commits](https://www.conventionalcommits.org/). The release changelog
is built from them: `feat:`, `fix:` and `perf:` each get a section;
`refactor:`, `test:`, `docs:`, `build:`, `ci:` and `chore:` are left
out, since they change nothing a user sees.

## Releases

`make next-version` prints the next tag: a breaking change (`type!:`
or a `BREAKING CHANGE:` footer) bumps major, a `feat:` minor, a `fix:`
or `perf:` patch. It fails when the commits since the last tag hold
none of those. `make release-notes` prints the changelog that tag
would carry. `make release-tag` tags HEAD with it and prints the push
that releases it:

    make release-tag
    git push origin v1.2.3

CI builds linux and darwin, amd64 and arm64, and publishes a GitHub
release with archives, `checksums.txt` and a changelog from the commit
messages. Each archive holds the completion scripts of
`internal/cli/completions/` in `completions/`. `vedi -v` prints the tag; `go install ...@v1.2.3` builds
print the same from module data.

Darwin binaries are signed with a Developer ID Application certificate
and notarized (bare binaries can't be stapled; Gatekeeper checks the
ticket online). Both steps are skipped when the secrets are absent.
The secrets live in the GitHub `release` environment, which only the
release workflow on a `v*` tag can use. The API key comes from App
Store Connect: Users and Access → Integrations → Team Keys, Developer
role.

    APPLE_CERTIFICATE_P12_BASE64   the certificate, base64
    APPLE_CERTIFICATE_PASSWORD     its password
    APPLE_API_ISSUER_ID            API key issuer ID
    APPLE_API_KEY_ID               API key ID
    APPLE_API_KEY_P8_BASE64        API key .p8, base64

Export the certificate from Keychain Access (My Certificates →
Developer ID Application → File → Export Items… → .p12). Encode both
files without line breaks; goreleaser rejects wrapped base64:

    base64 -i Certificates.p12 | tr -d ' \n' | pbcopy
    base64 -i AuthKey_XXXXXXXXXX.p8 | tr -d ' \n' | pbcopy

To load them from a 1Password item whose fields carry the same names:

    bin/sync_secrets_from_1password.sh "op://Private/GitHub vedi Secrets"

To try a release locally without tagging:

    go tool -modfile=tools/go.mod goreleaser release --snapshot --clean

Output lands in `dist/`, which is ignored.
