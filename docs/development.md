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

To try a release locally without tagging:

    go tool -modfile=tools/go.mod goreleaser release --snapshot --clean

Output lands in `dist/`, which is ignored.
