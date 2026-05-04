#!/usr/bin/env bash
# Smoke test for cool-mcp: send init handshake, list tools, call whoami.
# Reads JSON-RPC responses from stdout, prints them to stderr for inspection.

set -e

BIN="${1:-./target/debug/cool-mcp}"

if [ ! -x "$BIN" ]; then
  echo "binary not found: $BIN" >&2
  exit 1
fi

# Send 4 messages then close stdin so the server exits.
{
  printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"smoke","version":"0"}}}'
  printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/initialized"}'
  printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"tools/list"}'
  printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"whoami","arguments":{}}}'
  # Give the server a beat to respond before we close stdin.
  sleep 1
} | "$BIN" 2>/tmp/cool-mcp.stderr
