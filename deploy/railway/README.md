# Railway Deployment for mooR

This directory contains configuration for deploying mooR to [Railway](https://railway.app/).

## Features

- **Persistent Volume**: Stores the core database and all player data
- **Multi-service Container**: Runs daemon, web host, telnet host, curl worker, and frontend
- **Exposed Ports**:
  - `8080` - Frontend web UI (Meadow) + API proxy (main public endpoint)
  - `8081` - Direct web API access
  - `8888` - Telnet interface (optional, for traditional MUD clients)

## MCP Access

The MCP (Model Context Protocol) host is available for AI agent integration. To use MCP:

1. **Via Railway Private Service**: Add a separate Railway service for MCP that connects to the main service's RPC ports
2. **Via SSH**: SSH into the running container and invoke `moor-mcp-host` directly
3. **Via stdio over Railway Proxy**: Configure your MCP client to use Railway's proxy

The MCP host runs in `session-only` mode by default, requiring agents to authenticate via `moo_session_login` or `moo_session_create`.

## Setup Instructions

### 1. Install Railway CLI

```bash
npm install -g @railway/cli
```

### 2. Login to Railway

```bash
railway login
```

### 3. Create a New Project

```bash
railway init
```

Follow the prompts to create a new project.

### 4. Add a Persistent Volume

In the Railway dashboard or via CLI:

```bash
railway volume create data
```

Or in the dashboard: Settings > Volumes > Create Volume
- Name: `data`
- Mount Path: `/data`

### 5. Configure Environment Variables

The `railway.toml` file includes default environment variables. You can override these in the dashboard:

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | `8080` | Main web port |
| `RUN_DIR` | `run-cowbell` | Runtime subdirectory |
| `BUILD_PROFILE` | `release-fast` | Rust build profile |
| `IMPORT_PATH` | `/data/cores/cowbell/src` | Core import path |
| `USE_BOOLEAN_RETURNS` | `true` | Feature flag |
| `CUSTOM_ERRORS` | `true` | Feature flag |
| `USE_UUOBJIDS` | `true` | Feature flag |
| `ANONYMOUS_OBJECTS` | `true` | Feature flag |
| `ENABLE_EVENTLOG` | `true` | Feature flag |
| `MCP_CREATION_POLICY` | `open` | MCP account creation |

### 6. Deploy

```bash
railway up
```

Or connect your GitHub repository and deploy from there.

## Post-Deployment Setup

### 1. Access Your Instance

Once deployed, Railway will provide a URL like `https://your-project.up.railway.app`

### 2. Initialize the Cowbell Core

On first deployment, the Cowbell core needs to be fetched. The entrypoint script attempts to do this automatically, but you may need to:

1. SSH into the running container (Railway supports this)
2. Run: `/app/fetch-cowbell.sh` if available
3. Or restart the deployment to trigger initialization

### 3. Configure Public Access

In Railway dashboard > Settings > Networking:
- Enable public networking for ports 8080 and 8888

### 4. Set Up Custom Domain (Optional)

In Settings > Custom Domains, add your own domain.

## Monitoring

- View logs in the Railway dashboard or via `railway logs`
- Health check: `https://your-project.up.railway.app/health`

## Persistent Data

All persistent data is stored in `/data` on the mounted volume:
- `/data/run-cowbell/moor-data/development.db` - Main database
- `/data/run-cowbell/export/` - Exports
- `/data/run-cowbell/config/` - Keys and config
- `/data/cores/cowbell/src/` - Core MOO code

## Troubleshooting

### Database Not Found

If the daemon fails to start due to missing database, the first run will create it automatically.

### Cowbell Core Missing

Ensure `cores/fetch-cowbell.sh` exists in your repository or add the core files to a persistent volume before first start.

### Out of Memory

If you encounter OOM errors, increase the RAM allocation in Settings > Plans.

## Costs

Railway charges based on usage:
- Free tier includes $5/month credit
- Persistent volumes incur additional cost
- See https://railway.app/pricing for details
