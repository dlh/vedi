# Development

Requires Go and make.

## Checks

    make test           vet, staticcheck, go test ./...
    make fmt            list unformatted files (fails if any)
    make tidy           show what go mod tidy would change (fails if any)
    make bench          benchmarks
    make release-check  validate .goreleaser.yaml (needs a git remote)

CI runs `fmt`, `test` and `tidy` on every push and PR;
run them before committing. Behaviors are specified and tested in
`specs/`; see `specs/README.md`.

## Releases

Tag and push:

    git tag v1.2.3
    git push origin v1.2.3

CI builds linux and darwin, amd64 and arm64, and publishes a GitHub
release with archives, `checksums.txt` and a changelog from the commit
messages. `vedi -v` prints the tag; `go install ...@v1.2.3` builds
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
