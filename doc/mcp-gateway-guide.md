# MCP Gateway Usage Guide

This guide explains how to interact with a mooR MOO server via the MCP (Model Context Protocol) gateway. The MCP gateway exposes the MOO as a set of JSON-RPC tools that can be called over HTTP, making it easy for AI agents and other programs to interact with the MOO world.

## Quick Start for AI Agents

### Connecting and Authenticating

**Endpoint**: `https://<your-railway-domain>/mcp/mcp`

The MCP gateway uses HTTP with JSON-RPC. Follow this sequence:

1. **Initialize** to get a session ID
2. **Create or Login** to a MOO player account
3. **Use the session** for subsequent tool calls

### Required Headers

All POST requests to the MCP gateway must include these headers:

```http
Content-Type: application/json
Accept: application/json
MCP-Protocol-Version: 2025-11-25
Mcp-Session-Id: <session-id-from-initialize>
```

### Step 1: Initialize (Get Session ID)

Send an `initialize` request. The server will respond with a session ID in the response headers.

```bash
curl -i -X POST \
  -H "Content-Type: application/json" \
  -H "Accept: application/json" \
  -H "MCP-Protocol-Version: 2025-11-25" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "initialize",
    "params": {
      "protocolVersion": "2024-11-05",
      "capabilities": {},
      "clientInfo": {"name": "my-agent", "version": "1.0"}
    }
  }' \
  https://<your-domain>/mcp/mcp
```

**Response** (check the `mcp-session-id` header):
```http
HTTP/2 200
mcp-session-id: de7a8b9080da4646af07fec0239808cc
content-type: application/json

{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "protocolVersion": "2024-11-05",
    "capabilities": {...},
    "serverInfo": {"name": "moor-mcp-host", "version": "1.26.0"}
  }
}
```

**Save the `mcp-session-id` header value** - you'll need it for all subsequent requests!

### Step 2: Create a Player Account

Use `moo_session_create` to create a new player account and establish a MOO session.

```bash
curl -X POST \
  -H "Content-Type: application/json" \
  -H "Accept: application/json" \
  -H "MCP-Protocol-Version: 2025-11-25" \
  -H "Mcp-Session-Id: <your-session-id>" \
  -d '{
    "jsonrpc": "2.0",
    "id": 2,
    "method": "tools/call",
    "params": {
      "name": "moo_session_create",
      "arguments": {
        "username": "my_player",
        "password": "secure_password"
      }
    }
  }' \
  https://<your-domain>/mcp/mcp
```

**Response**:
```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "content": [{
      "type": "text",
      "text": "Session created.\n  session_id: c929b44a-7b71-4987-8b0b-2bfc13745802\n  player: #00007D-9C3E5A28B6\n  connect_type: Created"
    }],
    "isError": false
  }
}
```

The returned `session_id` is your **MOO player session ID** - this is different from the MCP session ID in the header. This MOO session ID is now set as your "current" session for tool calls.

**Note**: If you already have a player account, use `moo_session_login` instead:

```json
{
  "method": "tools/call",
  "params": {
    "name": "moo_session_login",
    "arguments": {
      "username": "existing_player",
      "password": "their_password"
    }
  }
}
```

### Step 3: Interact with the MOO

Now you can use MOO commands and eval:

**Execute a MOO command** (like typing in the game):
```bash
curl -X POST \
  -H "Content-Type: application/json" \
  -H "Accept: application/json" \
  -H "MCP-Protocol-Version: 2025-11-25" \
  -H "Mcp-Session-Id: <your-mcp-session-id>" \
  -d '{
    "jsonrpc": "2.0",
    "id": 3,
    "method": "tools/call",
    "params": {
      "name": "moo_command",
      "arguments": {"command": "look"}
    }
  }' \
  https://<your-domain>/mcp/mcp
```

**Response**:
```json
{
  "result": {
    "content": [{
      "type": "text",
      "text": "{The First Room\n\nYou are in the very First Room...}\n\n=> true"
    }]
  }
}
```

## Available Tools

The MCP gateway exposes 54 tools organized into categories:

### Session Management

| Tool | Description |
|------|-------------|
| `moo_session_create` | Create a new player account and session |
| `moo_session_login` | Log in to an existing player account |
| `moo_session_use` | Set the active session for subsequent calls |
| `moo_session_close` | Close and disconnect a session |
| `moo_sessions_list` | List all active player sessions |
| `moo_session_events` | Poll for narrative events from the MOO world |

### MOO Interaction

| Tool | Description |
|------|-------------|
| `moo_command` | Execute a MOO command as the player (like typing in-game) |
| `moo_eval` | Evaluate MOO code and return the result |
| `moo_invoke_verb` | Directly invoke a verb on an object with arguments |

### Object Manipulation

| Tool | Description |
|------|-------------|
| `moo_list_objects` | List objects in the MOO database |
| `moo_resolve` | Resolve an object reference to get detailed information |
| `moo_create_object` | Create a new object with a specified parent |
| `moo_recycle_object` | Destroy an object permanently |
| `moo_move_object` | Move an object to a new location |
| `moo_set_parent` | Change an object's parent |
| `moo_object_graph` | Show the inheritance graph for an object |
| `moo_object_flags` | Get an object's flags (player, programmer, wizard, etc.) |
| `moo_set_object_flag` | Set an object flag |

### Verb Management

| Tool | Description |
|------|-------------|
| `moo_list_verbs` | List all verbs defined on an object |
| `moo_get_verb` | Get a verb's source code and metadata |
| `moo_program_verb` | Program (compile and save) a verb with new MOO code |
| `moo_apply_patch_verb` | Apply a unified diff patch to a verb's source code |
| `moo_add_verb` | Add a new verb to an object |
| `moo_delete_verb` | Delete a verb from an object |
| `moo_set_verb_info` | Set a verb's metadata (owner, permissions, names) |
| `moo_set_verb_args` | Set a verb's argument specification |
| `moo_find_verb_definition` | Find where a verb is defined in the inheritance chain |

### Property Management

| Tool | Description |
|------|-------------|
| `moo_list_properties` | List all properties defined on an object |
| `moo_get_property` | Get the value of a property on an object |
| `moo_set_property` | Set the value of a property on an object |
| `moo_add_property` | Add a new property to an object |
| `moo_delete_property` | Delete a property from an object |

### Server Administration

| Tool | Description |
|------|-------------|
| `moo_connected_players` | List all currently connected players |
| `moo_server_info` | Get server information (version, uptime, memory) |
| `moo_queued_tasks` | List all queued (suspended) tasks |
| `moo_kill_task` | Kill a running or suspended task by ID |
| `moo_reconnect` | Reconnect to the mooR daemon |

### Wizard-Only Tools

The following tools require wizard privileges (use `wizard: true` parameter):

| Tool | Description |
|------|-------------|
| `moo_dispatch_command_verb` | Dispatch a command verb using parsed command spec |
| `moo_dump_object` | Dump an object to objdef format (text representation) |
| `moo_load_object` | Load an object from objdef format |
| `moo_reload_object` | Reload an existing object from objdef format |
| `moo_apply_patch_objdef` | Apply a unified diff patch to an object's objdef |
| `moo_read_objdef_file` | Read an objdef file from the filesystem |
| `moo_write_objdef_file` | Write an objdef file to the filesystem |
| `moo_load_objdef_file` | Load an object from an objdef file |
| `moo_reload_objdef_file` | Reload an object from an objdef file |
| `moo_diff_object` | Compare a database object with an objdef file |

### Utility Tools

| Tool | Description |
|------|-------------|
| `moo_function_help` | Get documentation for a MOO builtin function |
| `moo_test_compile` | Compile MOO code without executing it |
| `moo_parse_command` | Parse a command string using the built-in command parser |
| `moo_parse_command_for_player` | Parse a command using the player's match environment |
| `moo_find_command_verb` | Find command verbs matching a parsed command spec |
| `moo_list_prepositions` | List all valid prepositions for command parsing |
| `moo_notify` | Send a notification message to a connected player |

## Tool Parameter Reference

### Common Parameters

Many tools support these optional parameters:

| Parameter | Type | Description |
|-----------|------|-------------|
| `session_id` | string (UUID) | Execute as a specific player session instead of current |
| `wizard` | boolean | Execute with wizard privileges (dangerous!) |

### moo_command

Execute a MOO command as the player.

```json
{
  "name": "moo_command",
  "arguments": {
    "command": "look",           // Required: The command to execute
    "session_id": "...",         // Optional: Override session
    "wizard": false              // Optional: Use wizard mode
  }
}
```

### moo_eval

Evaluate MOO code and return the result.

**Important**: You must use an explicit `return` statement to get a value back.

```json
{
  "name": "moo_eval",
  "arguments": {
    "expression": "return 1 + 2;",  // Required: MOO code to evaluate
    "session_id": "...",            // Optional: Override session
    "wizard": false                 // Optional: Use wizard mode
  }
}
```

### moo_session_create

Create a new player account and session.

```json
{
  "name": "moo_session_create",
  "arguments": {
    "username": "new_player",     // Required: Desired username
    "password": "secure_pass"     // Required: Password for the account
  }
}
```

### moo_session_login

Log in to an existing player account.

```json
{
  "name": "moo_session_login",
  "arguments": {
    "username": "existing_player",  // Required: Username
    "password": "their_password"    // Required: Password
  }
}
```

### moo_session_use

Set the current active session.

```json
{
  "name": "moo_session_use",
  "arguments": {
    "session_id": "c929b44a-7b71-4987-8b0b-2bfc13745802"  // Required: Session UUID
  }
}
```

### moo_get_verb

Get a verb's source code and metadata.

```json
{
  "name": "moo_get_verb",
  "arguments": {
    "object": "#123",              // Required: Object reference
    "verb": "describe",            // Required: Verb name
    "session_id": "...",           // Optional: Override session
    "wizard": false                // Optional: Use wizard mode
  }
}
```

### moo_program_verb

Program (compile and save) a verb with new MOO code.

```json
{
  "name": "moo_program_verb",
  "arguments": {
    "object": "#123",              // Required: Object reference
    "verb": "describe",            // Required: Verb name
    "code": "return \"A description\";",  // Required: MOO code
    "session_id": "...",           // Optional: Override session
    "wizard": false                // Optional: Use wizard mode
  }
}
```

### moo_list_objects

List objects in the MOO database.

```json
{
  "name": "moo_list_objects",
  "arguments": {
    "parent": "#0",                // Optional: Parent object to list under
    "recursive": false,            // Optional: List recursively
    "include_details": false       // Optional: Include full object details
  }
}
```

## Response Format

All tool calls return responses in this format:

```json
{
  "jsonrpc": "2.0",
  "id": <request-id>,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "<output from the MOO>"
      }
    ],
    "isError": false
  }
}
```

Error responses:
```json
{
  "jsonrpc": "2.0",
  "id": <request-id>,
  "result": {
    "content": [{
      "type": "text",
      "text": "<error message>"
    }],
    "isError": true
  }
}
```

## Common MOO Commands

Here are some useful commands to use with `moo_command`:

| Command | Description |
|---------|-------------|
| `look` | Look at the current room |
| `look <object>` | Look at a specific object |
| `inventory` or `inv` | List what you're carrying |
| `get <object>` | Pick up an object |
| `drop <object>` | Drop an object |
| `go <direction>` | Move in a direction (north, south, etc.) |
| `say <message>` | Say something out loud |
| `whisper <player> = <message>` | Whisper to a player |
| `emote <action>` | Perform an emote |
| `@examine <object>` | Examine an object's verbs and properties |
| `@quit` | Disconnect from the MOO |

## Python Example

```python
import requests
import json

class MOOMCPClient:
    def __init__(self, base_url):
        self.base_url = base_url
        self.session_id = None
        self.moo_session_id = None

    def initialize(self):
        """Initialize connection and get MCP session ID"""
        response = requests.post(
            f"{self.base_url}/mcp/mcp",
            headers={
                "Content-Type": "application/json",
                "Accept": "application/json",
                "MCP-Protocol-Version": "2025-11-25"
            },
            json={
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {"name": "python-client", "version": "1.0"}
                }
            }
        )
        self.session_id = response.headers["mcp-session-id"]
        return response.json()

    def call_tool(self, tool_name, arguments):
        """Call an MCP tool"""
        if not self.session_id:
            raise Exception("Not initialized. Call initialize() first.")

        response = requests.post(
            f"{self.base_url}/mcp/mcp",
            headers={
                "Content-Type": "application/json",
                "Accept": "application/json",
                "MCP-Protocol-Version": "2025-11-25",
                "Mcp-Session-Id": self.session_id
            },
            json={
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": tool_name,
                    "arguments": arguments
                }
            }
        )
        return response.json()

    def create_player(self, username, password):
        """Create a new player account"""
        result = self.call_tool("moo_session_create", {
            "username": username,
            "password": password
        })
        # Extract session ID from response text
        text = result["result"]["content"][0]["text"]
        # Parse "session_id: <uuid>" from text
        for line in text.split("\n"):
            if "session_id:" in line:
                self.moo_session_id = line.split("session_id:")[1].strip()
        return result

    def command(self, cmd):
        """Execute a MOO command"""
        return self.call_tool("moo_command", {"command": cmd})

    def look(self):
        """Look around the current room"""
        return self.command("look")

    def inventory(self):
        """Check inventory"""
        return self.command("inventory")

# Usage
client = MOOMCPClient("https://<your-railway-domain>")
client.initialize()
client.create_player("my_player", "password123")
print(client.look())
```

## JavaScript/TypeScript Example

```typescript
interface MCPResponse {
  jsonrpc: string;
  id: number;
  result?: {
    content: Array<{ type: string; text: string }>;
    isError: boolean;
  };
  error?: {
    code: number;
    message: string;
  };
}

class MOOMCPClient {
  private baseUrl: string;
  private mcpSessionId: string | null = null;
  private mooSessionId: string | null = null;
  private requestId = 0;

  constructor(baseUrl: string) {
    this.baseUrl = baseUrl;
  }

  async initialize(): Promise<MCPResponse> {
    const response = await fetch(`${this.baseUrl}/mcp/mcp`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Accept': 'application/json',
        'MCP-Protocol-Version': '2025-11-25'
      },
      body: JSON.stringify({
        jsonrpc: '2.0',
        id: ++this.requestId,
        method: 'initialize',
        params: {
          protocolVersion: '2024-11-05',
          capabilities: {},
          clientInfo: { name: 'ts-client', version: '1.0' }
        }
      })
    });

    this.mcpSessionId = response.headers.get('mcp-session-id');
    return response.json();
  }

  async callTool(toolName: string, arguments: Record<string, any>): Promise<MCPResponse> {
    if (!this.mcpSessionId) {
      throw new Error('Not initialized. Call initialize() first.');
    }

    const response = await fetch(`${this.baseUrl}/mcp/mcp`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Accept': 'application/json',
        'MCP-Protocol-Version': '2025-11-25',
        'Mcp-Session-Id': this.mcpSessionId
      },
      body: JSON.stringify({
        jsonrpc: '2.0',
        id: ++this.requestId,
        method: 'tools/call',
        params: {
          name: toolName,
          arguments
        }
      })
    });

    return response.json();
  }

  async createPlayer(username: string, password: string): Promise<MCPResponse> {
    const result = await this.callTool('moo_session_create', { username, password });
    // Extract session ID from response
    const text = result.result?.content[0]?.text || '';
    const match = text.match(/session_id:\s+([a-f0-9-]+)/i);
    if (match) {
      this.mooSessionId = match[1];
    }
    return result;
  }

  async command(cmd: string): Promise<MCPResponse> {
    return this.callTool('moo_command', { command: cmd });
  }

  async look(): Promise<MCPResponse> {
    return this.command('look');
  }

  async inventory(): Promise<MCPResponse> {
    return this.command('inventory');
  }
}

// Usage
const client = new MOOMCPClient('https://<your-railway-domain>');
await client.initialize();
await client.createPlayer('my_player', 'password123');
const room = await client.look();
console.log(room.result?.content[0]?.text);
```

## Complete Tool Reference

To get the complete list of tools with their full schemas:

```bash
curl -X POST \
  -H "Content-Type: application/json" \
  -H "Accept: application/json" \
  -H "MCP-Protocol-Version: 2025-11-25" \
  -H "Mcp-Session-Id: <your-session-id>" \
  -d '{
    "jsonrpc": "2.0",
    "id": 2,
    "method": "tools/list"
  }' \
  https://<your-domain>/mcp/mcp
```

This returns all 54 tools with their complete input schemas, descriptions, and parameter requirements.

## Troubleshooting

### "Session not found"
- The MCP session ID in your header may have expired
- Call `initialize` again to get a new session ID

### "No active session. Use moo_session_login or moo_session_create first"
- You need to create or log in to a player account before using most tools
- Use `moo_session_create` or `moo_session_login`

### "Not Acceptable: Client must accept text/event-stream"
- You sent a GET request without the proper Accept header
- Use POST with `Accept: application/json` instead

### "Not Acceptable: Client must accept application/json"
- You're trying to use SSE (Server-Sent Events) mode
- Use `Accept: application/json` for standard JSON-RPC requests

## Architecture

The MCP gateway consists of:

```
┌─────────────────────────────────────────────────────────────┐
│                     Railway Frontend                        │
│                      (nginx :80)                            │
│  Location: /mcp/ ──► proxy_pass ──► mcp-gateway :8090      │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                     MCP Gateway                             │
│              (mcp-proxy + moor-mcp-host)                    │
│  - Translates HTTP JSON-RPC to stdio MCP protocol           │
│  - Manages session state                                    │
│  - Routes tool calls to moor-mcp-host                       │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    mooR Daemon                              │
│           (ZMQ RPC over TCP :7899)                          │
│  - Core MOO database and execution engine                   │
│  - Manages player connections and sessions                  │
└─────────────────────────────────────────────────────────────┘
```

## References

- [MCP Specification](https://modelcontextprotocol.io/specification)
- [mooR Documentation](https://github.com/tigwyk/moor)
- [Railway Deployment Guide](./railway-deployment.md)
