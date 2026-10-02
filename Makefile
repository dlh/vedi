.PHONY: build install test bench bench-compare bench-pagers media vet staticcheck fix fmt tidy release-check next-version release-notes clean

build:
	go build -o vedi ./cmd/vedi

install:
	go install ./cmd/vedi

test: vet staticcheck fix
	go test ./...
	cd bench && go test ./...

bench:
	go test -run '^$$' -bench . -benchmem ./...

BENCH_GIT_BRANCH ?= main
BENCH_COUNT ?= 10

# Benchmarks at BENCH_GIT_BRANCH vs the working tree, BENCH_COUNT runs each.
bench-compare:
	bin/bench-compare $(BENCH_GIT_BRANCH) $(BENCH_COUNT)

# Needs less on PATH; takes a few minutes. BENCHFLAGS: -lines, -runs.
bench-pagers: build
	cd bench && go run . -vedi $(CURDIR)/vedi $(BENCHFLAGS)

# Needs vhs on PATH (brew install vhs). Rewrites docs/media/demo.gif.
media: build
	vhs docs/media/demo.tape

vet:
	go vet ./...
	cd bench && go vet ./...

staticcheck:
	go tool -modfile=tools/go.mod staticcheck ./...
	cd bench && go tool -modfile=../tools/go.mod staticcheck ./...

# Shows what go fix would change and fails; never rewrites.
fix:
	go fix -diff ./...
	cd bench && go fix -diff ./...

# Lists unformatted files and fails; never rewrites.
fmt:
	@out=$$(gofmt -l .); if [ -n "$$out" ]; then echo "$$out"; exit 1; fi

# Shows what go mod tidy would change and fails; never rewrites.
tidy:
	go mod tidy -diff
	cd tools && go mod tidy -diff
	cd bench && go mod tidy -diff

# Needs a git remote.
release-check:
	go tool -modfile=tools/go.mod goreleaser check

# Prints the next tag, from the commits since the last one.
next-version:
	@bin/next-version

# Prints the changelog the next release would carry.
release-notes:
	@bin/release-notes

clean:
	rm -f vedi
