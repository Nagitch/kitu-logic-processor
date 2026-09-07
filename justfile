# Common verification commands; general checks run in the Dev Container.
# Each attempt retains a new report under .tmp/verification/.
verify scope="all":
    python3 tools/verify-repository.py --scope "{{scope}}" --evidence ".tmp/verification/{{scope}}-$(date -u +%Y%m%dT%H%M%S)-$$$$"

# This is the only recipe that intentionally reformats source.
fmt:
    cargo fmt --all

fmt-check: (verify "fmt")
lint: (verify "clippy")
test: (verify "test")
doc: (verify "docs")
data: (verify "data")
frontend: (verify "frontend")
check-all: (verify "all")

# macOS + Apple SDK; full also requires the pinned licensed Unity Editor.
native evidence:
    python3 tools/verify-arena-macos.py --scope native --evidence "{{evidence}}"

unity evidence:
    python3 tools/verify-arena-macos.py --scope full --evidence "{{evidence}}"

build:
    cargo build --locked --workspace
