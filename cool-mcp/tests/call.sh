#!/usr/bin/env bash
# One-shot JSON-RPC tool invoker for cool-mcp.
# Spawns the binary, runs init handshake, calls one tool, prints the result.
#
# Usage:
#   call.sh <tool> [json_args]
# Examples:
#   call.sh whoami
#   call.sh courses_list '{"all": false}'
#   call.sh courses_resolve '{"query": "Embodied"}'

set -e

TOOL="${1:?usage: call.sh <tool> [json_args]}"
ARGS="${2:-{\}}"

REPO="$(cd "$(dirname "$0")/../../.." && pwd)"
BIN="${COOL_MCP_BIN:-$REPO/v2/target/release/cool-mcp}"

if [ ! -x "$BIN" ]; then
  echo "binary not found: $BIN — run cargo build --release -p cool-mcp" >&2
  exit 1
fi

# Build the 4-message exchange: init, init-notify, tool call, then EOF.
{
  printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"call.sh","version":"0"}}}'
  printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/initialized"}'
  printf '%s\n' "$(jq -nc --arg t "$TOOL" --argjson a "$ARGS" \
    '{jsonrpc:"2.0", id:2, method:"tools/call", params:{name:$t, arguments:$a}}')"
  sleep 0.5
} | "$BIN" 2>/dev/null \
  | jq -c 'select(.id == 2) | .result.content[0].text | fromjson? // .'
