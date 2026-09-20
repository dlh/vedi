.PHONY: build install test bench vet staticcheck fmt release-check clean

build:
	go build -o vedi ./cmd/vedi

install:
	go install ./cmd/vedi

test: vet staticcheck
	go test ./...

bench:
	go test -run '^$$' -bench . -benchmem ./...

vet:
	go vet ./...

staticcheck:
	go tool -modfile=tools/go.mod staticcheck ./...

# Lists unformatted files and fails; never rewrites.
fmt:
	@out=$$(gofmt -l .); if [ -n "$$out" ]; then echo "$$out"; exit 1; fi

# Needs a git remote.
release-check:
	go tool -modfile=tools/go.mod goreleaser check

clean:
	rm -f vedi
