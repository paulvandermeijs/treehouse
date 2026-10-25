# MCP servers

Pi reads MCP servers from `~/.pi/agent/mcp.json` and, in [trusted](./trust) projects, from the project's `.pi/mcp.json`. After changing either, run `/reload` in the chat.

Pi reaches MCP tools through its `codemode` and `tool_search` tools, so Treehouse adds those to the tools it gives Pi. The tools a script calls still go through [approval](./approvals).

## Signing in

When a server needs a sign-in, the chat shows it with a **Sign in** button. That runs Pi's `/mcp login <server>`: Pi opens your browser, and the card waits for the sign-in to finish.

If the browser can't reach this machine (in a remote session, for example), copy the address the browser ended up on, paste it into the card and click **Use address**.

## Servers that refuse Pi

Some servers only accept sign-ins from apps they have approved, and refuse Pi with "dynamic client registration failed with status 403". Figma's remote server is one. For Figma, use the desktop app's local MCP server (`http://127.0.0.1:3845/mcp`) instead; it needs no sign-in.
