#!/usr/bin/env bash
# plugin-dev.sh — local dev/test helpers for the ntucool Claude Code plugin.
#
#   validate   check plugin.json + marketplace.json manifests (no session)
#   run        launch claude with the plugin loaded session-only (--plugin-dir);
#              fast inner loop for editing SKILL.md / commands / .mcp.json,
#              writes nothing to ~/.claude
#   install    launch claude in an isolated CLAUDE_CONFIG_DIR; exercises the
#              real marketplace-add + install UX without polluting ~/.claude
#   reset      delete the isolated config dir
#
# Extra args after the subcommand are passed through to claude.
# Override the isolated config dir with NTUCOOL_DEV_CONFIG=...
set -eu

REPO="$(cd "$(dirname "$0")/.." && pwd)"
PLUGIN_DIR="$REPO/plugins/ntucool"
DEV_CONFIG="${NTUCOOL_DEV_CONFIG:-$HOME/.claude-ntucool-dev}"

cmd="${1:-help}"
[ "$#" -gt 0 ] && shift

case "$cmd" in
  validate)
    claude plugin validate "$PLUGIN_DIR"
    claude plugin validate "$REPO"
    ;;
  run)
    exec claude --plugin-dir "$PLUGIN_DIR" "$@"
    ;;
  install)
    echo "Isolated config: $DEV_CONFIG"
    echo "Inside the session run:  /plugin marketplace add $REPO"
    echo "                         /plugin install ntucool@ntucool"
    exec env CLAUDE_CONFIG_DIR="$DEV_CONFIG" claude "$@"
    ;;
  reset)
    rm -rf "$DEV_CONFIG"
    echo "removed $DEV_CONFIG"
    ;;
  *)
    sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'
    ;;
esac
