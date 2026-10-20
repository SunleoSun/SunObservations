## SunMCP results in codemode

When calling SunMCP tools from codemode:

- Call tools with the `mcp__sunmcp__` prefix.
- Treat SunMCP text output as the canonical tool result. Render `content` text blocks directly and in order. Do not pass the whole `CallToolResult` to `text()` or otherwise JSON-serialize it unless the transport/result structure itself is being inspected.
- Preserve SunMCP status and diagnostics, including `Status`, warnings, truncation metadata, `Did_you_mean`, and partial/error information.
- Prefer SunMCP-native filtering and selection parameters over filtering, truncating, or reshaping results in codemode.
- Do not use codemode as a second rendering/filtering layer when the SunMCP contract can express the request directly.
- Keep `get_tool_info` focused: request only needed tools; use `full` only when exact field-level details or examples are required; tool names passed to SunMCP discovery are unprefixed.

Example:

```js
const result = await tools.mcp__sunmcp__read_memory_index({});
for (const block of result.content ?? []) {
  if (block.type === "text") text(block.text);
}
```