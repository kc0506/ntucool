# ntucool

NTU COOL (cool.ntu.edu.tw) CLI + MCP server.

NTU-only: login is NTU's ADFS SAML flow. Non-NTU Canvas instances aren't supported.

## Install

```sh
cargo install ntucool ntucool-mcp
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
