.PHONY: build install test bench bench-compare bench-pagers media fmt tidy release-check next-version changelog release-notes release-tag aur clean

build:
	cargo build --release -p vedi

install:
	cargo install --path vedi --locked

test:
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace

bench:
	cargo bench -p vedi

BENCH_GIT_BRANCH ?= main
BENCH_COUNT ?= 10

# Benchmarks at BENCH_GIT_BRANCH vs the working tree, BENCH_COUNT samples each.
bench-compare:
	bin/bench-compare $(BENCH_GIT_BRANCH) $(BENCH_COUNT)

# Needs less on PATH; takes a few minutes. BENCHFLAGS: --lines, --runs.
bench-pagers: build
	cargo run --release -q -p bench -- --vedi $(CURDIR)/target/release/vedi $(BENCHFLAGS)

# Needs vhs on PATH (brew install vhs), and bat for filter.tape. Rewrites
# docs/media/demo.gif and filter.gif. The config home holds no vedi.conf,
# so the demos run with the defaults.
media: build
	PATH="$(CURDIR)/target/release:$$PATH" XDG_CONFIG_HOME="$(CURDIR)/docs/media" vhs docs/media/demo.tape
	PATH="$(CURDIR)/target/release:$$PATH" XDG_CONFIG_HOME="$(CURDIR)/docs/media" vhs docs/media/filter.tape

# Shows what rustfmt would change and fails; never rewrites.
fmt:
	cargo fmt --all --check

# Fails when Cargo.lock is out of date; never rewrites.
tidy:
	cargo metadata --locked --format-version 1 >/dev/null

# Needs goreleaser on PATH (brew install goreleaser) and a git remote.
release-check:
	goreleaser check

# Prints the next tag, from the commits since the last one.
next-version:
	@bin/next-version

# Prepends the next release's entry to CHANGELOG.md, to edit before
# release-tag, which otherwise writes it.
changelog:
	@bin/changelog -w

# Prints the notes the next release carries: the first CHANGELOG.md entry.
release-notes:
	@bin/release-notes

# Writes the AUR packages to dist/aur/, for makepkg on Arch; needs goreleaser
# and gh.
aur:
	@bin/aur-package

# Writes and commits the CHANGELOG.md entry unless it is there, then tags
# HEAD with the next version; pushing the tag releases it.
release-tag:
	@bin/release-tag

clean:
	cargo clean
