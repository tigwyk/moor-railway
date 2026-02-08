# Deploying mooR to Railway

This guide covers deploying mooR to [Railway](https://railway.app/).

## Quick Start

1. **Install Railway CLI**
   ```bash
   npm install -g @railway/cli
   ```

2. **Login and Initialize**
   ```bash
   railway login
   railway init
   ```

3. **Create Persistent Volume**
   ```bash
   railway volume create data
   ```
   Set mount path to: `/data`

4. **Deploy**
   ```bash
   railway up
   ```

## Configuration

The `railway.toml` file in the repository root configures the deployment:

- **Dockerfile**: `Dockerfile.railway` - Multi-stage build including backend and frontend
- **Persistent Volume**: Mounted at `/data` for database, core files, and exports
- **Exposed Ports**:
  - `8080` - Main web interface (Meadow frontend + API proxy)
  - `8081` - Direct web API
  - `8888` - Telnet interface

## Architecture

The Railway deployment runs all services in a single container:

```
┌─────────────────────────────────────────────────────────┐
│  Railway Container                                      │
│                                                         │
│  ┌─────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │  nginx      │  │ moor-daemon  │  │moor-web-host │  │
│  │  (frontend) │  │  (backend)   │  │   (API)      │  │
│  │  :8080      │  │  :7899       │  │   :8081      │  │
│  └──────┬──────┘  └──────┬───────┘  └──────┬───────┘  │
│         │                │                  │          │
│         └────────────────┴──────────────────┘          │
│                         │                              │
│                  ┌──────▼──────┐                       │
│                  │  Persistent  │                       │
│                  │   Volume     │                       │
│                  │    /data     │                       │
│                  └──────────────┘                       │
└─────────────────────────────────────────────────────────┘
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | `8080` | Main web port |
| `RUN_DIR` | `run-cowbell` | Runtime subdirectory within data volume |
| `BUILD_PROFILE` | `release-fast` | Rust build profile |
| `IMPORT_PATH` | `/data/cores/cowbell/src` | Core MOO code location |
| `MCP_CREATION_POLICY` | `open` | MCP account creation policy |

## First Run

On first deployment, the entrypoint script will:

1. Create required directories in `/data`
2. Fetch the Cowbell core from Codeberg
3. Generate encryption keys
4. Initialize the database
5. Start all services (daemon, web-host, telnet, worker, nginx)

## Accessing Your Deployment

After deployment, Railway provides a public URL:

- **Web UI**: `https://your-project.up.railway.app`
- **Health Check**: `https://your-project.up.railway.app/health`
- **Telnet**: `telnet your-project.up.railway.app 8888`

## MCP Integration

For Model Context Protocol access, configure your MCP client to connect via Railway's private networking or SSH into the container to run `moor-mcp-host` directly.

See `deploy/railway/README.md` for detailed documentation.

## Local Testing

Test the Railway configuration locally:

```bash
docker compose -f docker-compose.railway.yml up --build
```

This uses the same Dockerfile and entrypoint script as the Railway deployment.
