# Railway Multi-Service Deployment Guide

This document captures hard-won lessons from deploying mooR to Railway as a
multi-service project using pre-built DockerHub images.

## Architecture

Six services communicate over Railway's private networking:

```
                    ┌──────────────────┐
   Internet ───────►│    frontend      │
   (HTTPS)          │  nginx :80       │
                    └────────┬─────────┘
                             │ proxy_pass
                    ┌────────┴─────────┐
                    │                  │
           ┌────────▼─────┐  ┌────────▼─────────┐
           │   webhost    │  │   mcp-gateway     │
           │ moor-web-host│  │ mcp-proxy :8090   │
           │   :8081      │  │   ↕ stdio         │
           └────────┬─────┘  │ moor-mcp-host     │
                    │        └────────┬───────────┘
                    │ TCP (ZMQ + CURVE)│
          ┌─────────┼─────────────────┤
          │         │                 │
 ┌────────▼───┐  ┌──▼────────────┐  ┌▼───────────────┐
 │   telnet   │  │    daemon     │  │    worker       │
 │ :8888      │  │  :7896-7900   │  │ moor-curl-worker│
 └────────────┘  │ (core DB+RPC) │  └─────────────────┘
                 └───────────────┘
```

All inter-service communication uses TCP with CURVE encryption via ZeroMQ.
Services discover each other using Railway private DNS:
`{service-name}.railway.internal`.

## Key Lessons

### 1. RAILWAY_DOCKERFILE_PATH is required for monorepos

Railway CLI `railway up` does **not** read `dockerfilePath` from `railway.toml`
when services were created via `railway add --image`. Instead, set the
`RAILWAY_DOCKERFILE_PATH` environment variable on each service:

```bash
railway variable set --service daemon --skip-deploys RAILWAY_DOCKERFILE_PATH=Dockerfile.daemon
railway variable set --service telnet --skip-deploys RAILWAY_DOCKERFILE_PATH=Dockerfile.telnet
# etc.
```

Without this, Railway's Railpack builder ignores your per-service Dockerfiles
and auto-detects the project language (finding Rust, failing to build).

### 2. The root Dockerfile interferes with Railway builds

If a root `Dockerfile` exists with features Railway doesn't support (like
`--mount=type=cache,target=...` without an `id=` parameter), Railway will
fail even when targeting a different Dockerfile. Railway requires cache mounts
in the format `--mount=type=cache,id=<service-id>,target=<path>`.

The `RAILWAY_DOCKERFILE_PATH` variable solves this by directing the builder
to the correct Dockerfile.

### 3. Heredoc COPY syntax is not supported

Railway's builder does not support Dockerfile heredoc syntax:

```dockerfile
# THIS DOES NOT WORK ON RAILWAY
COPY <<'ENTRYPOINT' /entrypoint.sh
#!/bin/bash
...
ENTRYPOINT
```

Use separate script files with standard COPY instead:

```dockerfile
COPY deploy/railway/daemon-entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh
```

### 4. .railwayignore must not exclude files needed by Dockerfiles

The `.railwayignore` file controls what gets uploaded via `railway up`. If your
Dockerfiles COPY files from `deploy/railway/`, that directory must not be in
`.railwayignore`. We had:

```
# BAD - this excluded our entrypoint scripts
deploy/railway/
```

### 5. MOOR_ENROLLMENT_TOKEN must be shared across services

The daemon generates a CURVE enrollment token on first boot. Other services
need this token to authenticate. In a multi-container setup (unlike
docker-compose with shared IPC sockets), you must explicitly share the token:

```bash
TOKEN=$(python3 -c "import uuid; print(uuid.uuid4())")
for svc in daemon telnet webhost worker; do
  railway variable set --service "$svc" --skip-deploys "MOOR_ENROLLMENT_TOKEN=$TOKEN"
done
```

The daemon's entrypoint writes this to the config directory so it uses the
same token across restarts. Other services read it from the environment
variable directly.

### 6. Railway PORT variable

Railway routes HTTP traffic to whatever port the `PORT` environment variable
specifies. For nginx listening on port 80:

```bash
railway variable set --service frontend PORT=80
```

Without this, Railway's edge proxy returns 502 because it can't find the
service's listening port.

### 7. Nginx upstream DNS resolution

Nginx resolves upstream hostnames at startup and caches them. If the upstream
service isn't running yet, nginx crashes with:

```
host not found in upstream "webhost.railway.internal:8081"
```

Two approaches to handle this:

**Option A: Static upstream (simpler, used here)**
```nginx
upstream moor_api {
    server webhost.railway.internal:8081;
}
```
This works if the webhost service is already running when frontend starts.
If frontend crashes, Railway's restart policy retries until webhost is up.

**Option B: Variable-based lazy resolution**
```nginx
resolver <dns-ip> valid=30s;
server {
    location /v1/ {
        set $backend http://webhost.railway.internal:8081;
        proxy_pass $backend;
    }
}
```
Note: `set` directives are only valid inside `server` or `location` blocks,
not in `http` block. And `127.0.0.11` (Docker's DNS) does not work on
Railway — use the system resolver from `/etc/resolv.conf` if you need this
approach.

### 8. The base DockerHub images have no entrypoint

The `tigwyk/moor-*:latest` images contain binaries at `/moor/` but have no
CMD or ENTRYPOINT set (CMD defaults to `bash`). Each Railway Dockerfile must
add an entrypoint script that:

- Creates required directories on the persistent volume
- (daemon only) Fetches the cowbell core if not present
- (daemon only) Writes the enrollment token to the config directory
- (non-daemon) Waits for the daemon's enrollment port to be reachable
- Starts the service binary with TCP addresses (not IPC sockets)

### 9. TCP vs IPC for inter-service communication

The docker-compose setup uses IPC sockets (`ipc:///var/run/moor/rpc.sock`).
Railway services are separate containers, so all communication must use TCP:

```bash
# Daemon listens on TCP
--rpc-listen=tcp://0.0.0.0:7899
--events-listen=tcp://0.0.0.0:7898

# Clients connect via Railway private DNS
--rpc-address=tcp://daemon.railway.internal:7899
--events-address=tcp://daemon.railway.internal:7898
--enrollment-address=tcp://daemon.railway.internal:7900
```

### 10. XDG directories on persistent volumes

Set `XDG_CONFIG_HOME` and `XDG_DATA_HOME` to paths within the persistent
volume so that keys, enrollment tokens, and host identity data survive
container restarts:

```dockerfile
ENV XDG_CONFIG_HOME=/data/config
ENV XDG_DATA_HOME=/data/local-share
```

## File Inventory

### Dockerfiles (project root)

| File | Base Image | Purpose |
|------|-----------|---------|
| `Dockerfile.daemon` | `tigwyk/moor-daemon:latest` | Adds git, fetch-cowbell.sh, daemon entrypoint |
| `Dockerfile.telnet` | `tigwyk/moor-telnet-host:latest` | Adds telnet entrypoint with daemon wait loop |
| `Dockerfile.webhost` | `tigwyk/moor-web-host:latest` | Adds webhost entrypoint with daemon wait loop |
| `Dockerfile.worker` | `tigwyk/moor-worker:latest` | Adds worker entrypoint with daemon wait loop |
| `Dockerfile.mcp-gateway` | `tigwyk/moor-daemon:latest` + `python:3.12-slim` | Installs mcp-proxy, copies moor-mcp-host, bridges stdio→HTTP |
| `Dockerfile.frontend` | `tigwyk/moor-frontend:latest` | Replaces nginx.conf with Railway-specific config |

### Entrypoint Scripts (deploy/railway/)

| File | Used By | Key Behavior |
|------|---------|-------------|
| `daemon-entrypoint.sh` | daemon | Creates dirs, fetches cowbell, writes enrollment token, starts moor-daemon with TCP listeners |
| `telnet-entrypoint.sh` | telnet | Waits for daemon, starts moor-telnet-host connecting via TCP |
| `webhost-entrypoint.sh` | webhost | Waits for daemon, starts moor-web-host connecting via TCP |
| `worker-entrypoint.sh` | worker | Waits for daemon, starts moor-curl-worker connecting via TCP |
| `mcp-gateway-entrypoint.sh` | mcp-gateway | Waits for daemon, starts mcp-proxy wrapping moor-mcp-host via TCP |
| `fetch-cowbell.sh` | daemon | Clones cowbell core from Codeberg into /data/cores/cowbell |
| `nginx-railway.conf` | frontend | Nginx config proxying to webhost.railway.internal |

### Configuration

| File | Purpose |
|------|---------|
| `railway.toml` | Monorepo service definitions (ports, volumes, env vars) |
| `.railwayignore` | Excludes large dirs (target/, crates/) from upload |

## Railway CLI Commands Reference

```bash
# Create a service from a Docker image
railway add --service <name> --image <image> --variables "KEY=VALUE"

# Set env vars (--skip-deploys prevents automatic redeploy)
railway variable set --service <name> --skip-deploys KEY=VALUE

# Delete an env var
railway variable delete --service <name> KEY

# Deploy from local source (uses RAILWAY_DOCKERFILE_PATH)
railway up --service <name> --detach

# Redeploy existing build
railway redeploy --service <name> --yes

# Check status
railway service link <name>  # must link first
railway service status

# View logs
railway logs                              # deploy logs for linked service
railway logs --build <deployment-id>      # build logs for specific deployment

# Add persistent volume
railway service link <name>
railway volume add --mount-path /data

# Add public domain
railway service link <name>
railway domain --port 80

# List all variables
railway variable list --service <name>
```

## Environment Variables Per Service

### daemon
| Variable | Value |
|----------|-------|
| `RAILWAY_DOCKERFILE_PATH` | `Dockerfile.daemon` |
| `IMPORT_PATH` | `/data/cores/cowbell/src` |
| `USE_BOOLEAN_RETURNS` | `true` |
| `CUSTOM_ERRORS` | `true` |
| `USE_UUOBJIDS` | `true` |
| `ANONYMOUS_OBJECTS` | `true` |
| `ENABLE_EVENTLOG` | `true` |
| `MOOR_ENROLLMENT_TOKEN` | `<shared-uuid>` |

### telnet, webhost
| Variable | Value |
|----------|-------|
| `RAILWAY_DOCKERFILE_PATH` | `Dockerfile.telnet` / `Dockerfile.webhost` |
| `DAEMON_HOST` | `daemon.railway.internal` |
| `DAEMON_RPC_PORT` | `7899` |
| `DAEMON_EVENTS_PORT` | `7898` |
| `DAEMON_ENROLLMENT_PORT` | `7900` |
| `MOOR_ENROLLMENT_TOKEN` | `<shared-uuid>` |

### worker
| Variable | Value |
|----------|-------|
| `RAILWAY_DOCKERFILE_PATH` | `Dockerfile.worker` |
| `DAEMON_HOST` | `daemon.railway.internal` |
| `DAEMON_RPC_PORT` | `7899` |
| `DAEMON_EVENTS_PORT` | `7898` |
| `DAEMON_WORKERS_REQUEST_PORT` | `7896` |
| `DAEMON_WORKERS_RESPONSE_PORT` | `7897` |
| `DAEMON_ENROLLMENT_PORT` | `7900` |
| `MOOR_ENROLLMENT_TOKEN` | `<shared-uuid>` |

### mcp-gateway
| Variable | Value |
|----------|-------|
| `RAILWAY_DOCKERFILE_PATH` | `Dockerfile.mcp-gateway` |
| `DAEMON_HOST` | `daemon.railway.internal` |
| `DAEMON_RPC_PORT` | `7899` |
| `DAEMON_EVENTS_PORT` | `7898` |
| `DAEMON_ENROLLMENT_PORT` | `7900` |
| `MCP_CREATION_POLICY` | `open` |
| `MOOR_ENROLLMENT_TOKEN` | `<shared-uuid>` |

### frontend
| Variable | Value |
|----------|-------|
| `RAILWAY_DOCKERFILE_PATH` | `Dockerfile.frontend` |
| `PORT` | `80` |
| `WEB_HOST_SERVICE_NAME` | `webhost` |
| `WEB_HOST_PORT` | `8081` |

## Ports

| Service | Port | Protocol | Exposure |
|---------|------|----------|----------|
| daemon | 7896 | TCP | Private (workers request) |
| daemon | 7897 | TCP | Private (workers response) |
| daemon | 7898 | TCP | Private (events pub-sub) |
| daemon | 7899 | TCP | Private (RPC) |
| daemon | 7900 | TCP | Private (enrollment) |
| telnet | 8888 | TCP | Public (TCP proxy) |
| webhost | 8081 | TCP | Private (API) |
| mcp-gateway | 8090 | TCP | Private (proxied via nginx /mcp/) |
| frontend | 80 | HTTP | Public (domain) |

## Troubleshooting

**502 Bad Gateway**: Check that `PORT` env var matches what your service
listens on. Check `railway logs` for nginx errors.

**"host not found in upstream"**: The upstream service isn't running or DNS
isn't resolving. Ensure the upstream service is healthy first, then redeploy
the frontend.

**"Not enrolled and no enrollment token provided"**: Set
`MOOR_ENROLLMENT_TOKEN` on all backend services with the same UUID value.

**"Cache mounts MUST be in format..."**: Set `RAILWAY_DOCKERFILE_PATH` to
avoid the root Dockerfile being used.

**Services exit immediately**: The base DockerHub images have no entrypoint.
Ensure the Dockerfile adds an entrypoint script.

**Build succeeds but COPY fails**: Check `.railwayignore` isn't excluding
the files your Dockerfile needs.
