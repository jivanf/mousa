# Mousa MCP

`mousa-mcp` lets a local MCP client search and manage Mousa while the desktop app is running. It communicates over standard input/output and never exposes Mousa on the network.

Build or install Mousa, then configure the MCP client to launch the installed `mousa-mcp` executable without arguments. Start Mousa before calling its tools. If Mousa is closed, tools return `app_not_running` and do not launch it automatically.

Example client configuration:

```json
{
  "mcpServers": {
    "mousa": {
      "command": "/home/you/.local/bin/mousa-mcp"
    }
  }
}
```

On Windows, use the full path to `mousa-mcp.exe` in the Mousa installation directory. Account credentials remain in the operating-system keyring and are never returned by MCP tools.
