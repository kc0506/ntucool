# ntucool

NTU COOL (cool.ntu.edu.tw) CLI + MCP server.

NTU-only: login is NTU's ADFS SAML flow. Non-NTU Canvas instances aren't supported.

## Install

Prebuilt binaries (Linux / macOS / Windows — no Rust needed):

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/kc0506/ntucool/releases/latest/download/ntucool-installer.sh | sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/kc0506/ntucool/releases/latest/download/ntucool-mcp-installer.sh | sh
```

Windows PowerShell:

```powershell
powershell -ExecutionPolicy ByPass -c "irm https://github.com/kc0506/ntucool/releases/latest/download/ntucool-installer.ps1 | iex"
powershell -ExecutionPolicy ByPass -c "irm https://github.com/kc0506/ntucool/releases/latest/download/ntucool-mcp-installer.ps1 | iex"
```

Or from source:

```sh
cargo install ntucool ntucool-mcp
```

Then:

```sh
cool login
```

## CLI

```sh
cool whoami
cool course list
cool grade
cool submission mine --status graded
cool file list --course 57439 --path /
```

`cool --help` for the full command surface.

## MCP

`ntucool-mcp` is a stdio MCP server. Claude Desktop / Cursor config:

```json
{
  "mcpServers": {
    "ntucool": {
      "command": "ntucool-mcp"
    }
  }
}
```

Tool surface: [`docs/TOOLS.md`](docs/TOOLS.md).

## License

MIT.
