#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

SERVER_URL="${SERVER_URL:-http://127.0.0.1:3000}"
MODULE_PATH="${MODULE_PATH:-modules/module-test}"
PROCEDURE_FILE="${PROCEDURE_FILE:-SleepOneSecondProcedure.swift}"
PROCEDURE_TYPE="${PROCEDURE_TYPE:-SleepOneSecondProcedure}"
DB_NAME="${DB_NAME:-swift-proc-e2e-$(date +%s)}"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: this script currently requires macOS (Darwin)" >&2
  exit 1
fi

if ! command -v spacetime >/dev/null 2>&1; then
  echo "error: spacetime CLI not found in PATH" >&2
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo not found in PATH" >&2
  exit 1
fi

if ! command -v swiftc >/dev/null 2>&1; then
  echo "error: swiftc not found in PATH" >&2
  exit 1
fi

MODULE_ABS="${REPO_ROOT}/${MODULE_PATH}"
if [[ ! -d "${MODULE_ABS}" ]]; then
  echo "error: module path not found: ${MODULE_ABS}" >&2
  exit 1
fi

OUT_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/spacetimedb-swift-proc-e2e.XXXXXX")"
GENERATED_DIR="${OUT_ROOT}/generated"
mkdir -p "${GENERATED_DIR}"

echo "==> Publishing module '${MODULE_PATH}' to '${SERVER_URL}' as database '${DB_NAME}'"
spacetime publish \
  -s "${SERVER_URL}" \
  --anonymous \
  -y \
  -p "${MODULE_ABS}" \
  "${DB_NAME}"

echo "==> Generating Swift bindings with in-repo CLI"
cargo run -q -p spacetimedb-cli --manifest-path "${REPO_ROOT}/Cargo.toml" -- \
  generate \
  --lang swift \
  --out-dir "${GENERATED_DIR}" \
  --module-path "${MODULE_ABS}" \
  --no-config

GENERATED_PROCEDURE_FILE="${GENERATED_DIR}/${PROCEDURE_FILE}"
if [[ ! -f "${GENERATED_PROCEDURE_FILE}" ]]; then
  echo "error: expected generated procedure file not found: ${GENERATED_PROCEDURE_FILE}" >&2
  echo "generated files:" >&2
  ls -1 "${GENERATED_DIR}" >&2
  exit 1
fi

RUNNER_FILE="${OUT_ROOT}/runner.swift"
RUNNER_BIN="${OUT_ROOT}/runner-bin"

cat > "${RUNNER_FILE}" <<EOF
import Foundation

@MainActor
final class E2EDelegate: SpacetimeClientDelegate {
    var connected = false
    func onConnect() { connected = true }
    func onDisconnect(error: Error?) {}
    func onIdentityReceived(identity: [UInt8], token: String) {}
    func onTransactionUpdate(message: Data?) {}
}

@MainActor
func waitUntil(_ timeoutSeconds: Double, condition: @escaping @MainActor () -> Bool) async -> Bool {
    let start = Date()
    while Date().timeIntervalSince(start) < timeoutSeconds {
        if condition() { return true }
        try? await Task.sleep(for: .milliseconds(50))
    }
    return condition()
}

@MainActor
func runE2E() async -> Int32 {
    let delegate = E2EDelegate()
    let client = SpacetimeClient(serverUrl: URL(string: "${SERVER_URL}")!, moduleName: "${DB_NAME}")
    SpacetimeClient.shared = client
    client.delegate = delegate
    client.connect(token: nil)

    guard await waitUntil(10.0, condition: { delegate.connected }) else {
        fputs("E2E FAIL: client did not connect within timeout\\n", stderr)
        return 1
    }

    let start = Date()
    var callbackResult: Result<Void, Error>?
    ${PROCEDURE_TYPE}.invoke { result in
        callbackResult = result
    }

    guard await waitUntil(20.0, condition: { callbackResult != nil }), let result = callbackResult else {
        fputs("E2E FAIL: generated procedure callback was not received within timeout\\n", stderr)
        return 1
    }

    switch result {
    case .success:
        let elapsed = Date().timeIntervalSince(start)
        let elapsedString = String(format: "%.2f", elapsed)
        print("E2E OK: generated ${PROCEDURE_TYPE} callback succeeded in \\(elapsedString)s")
        client.disconnect()
        return 0
    case .failure(let error):
        fputs("E2E FAIL: generated procedure callback returned error: \\(error)\\n", stderr)
        client.disconnect()
        return 1
    }
}

@main
struct Runner {
    static func main() async {
        exit(await runE2E())
    }
}
EOF

echo "==> Compiling temporary Swift runner"
SDK_SOURCES=()
while IFS= read -r file; do
  SDK_SOURCES+=("${file}")
done < <(find "${REPO_ROOT}/sdks/swift/Sources/SpacetimeDB" -name '*.swift' | LC_ALL=C sort)

swiftc \
  "${SDK_SOURCES[@]}" \
  "${GENERATED_PROCEDURE_FILE}" \
  "${RUNNER_FILE}" \
  -o "${RUNNER_BIN}"

echo "==> Running E2E procedure callback check"
"${RUNNER_BIN}"

echo "==> Done"
echo "Artifacts kept in: ${OUT_ROOT}"
