# Common framework verification commands. Application and Unity verification
# live in Nagitch/kitu-unity-demo-game and are run by its own tooling.
verify scope="all":
    python3 tools/verify-repository.py --scope "{{scope}}" --evidence ".tmp/verification/{{scope}}-$(date -u +%Y%m%dT%H%M%S)-$$$$"

fmt:
    cargo fmt --all

fmt-check: (verify "fmt")
lint: (verify "clippy")
test: (verify "test")
doc: (verify "docs")
tools: (verify "tools")
frontend: (verify "frontend")
check-all: (verify "all")

build:
    cargo build --locked --workspace

# The framework gateway requires an application-owned Compose topology.
gateway-smoke compose:
    KITU_DEMO_COMPOSE_FILE="{{compose}}" tools/kitu-webtransport-gateway/scripts/smoke-in-docker.sh

gateway-integration compose:
    KITU_DEMO_COMPOSE_FILE="{{compose}}" tools/kitu-webtransport-gateway/scripts/integration-in-docker.sh
