# Railway Deployment using DockerHub Images

## Overview

Pre-built Docker images are available on DockerHub for all mooR services:
- `tigwyk/moor-daemon:latest` - Core MOO database and RPC server
- `tigwyk/moor-web-host:latest` - HTTP/WebSocket API server
- `tigwyk/moor-telnet-host:latest` - Telnet interface
- `tigwyk/moor-worker:latest` - Background HTTP request worker
- `tigwyk/moor-frontend:latest` - Meadow web UI (nginx)

## Railway Setup

For each service in Railway:

1. **Create New Service** → **Deploy from Docker Image**
2. **Image**: `tigwyk/moor-daemon:latest` (or respective image)
3. **Service Name**: `daemon`, `telnet`, `webhost`, `worker`, `frontend`
4. **Add Environment Variables** as needed per service

### Service Configuration

#### Daemon Service
- **Image**: `tigwyk/moor-daemon:latest`
- **Ports**: 7899 (TCP), 7898 (TCP), 7897 (TCP), 7896 (TCP), 7900 (TCP)
- **Volume**: Mount `/data` as persistent volume
- **Environment Variables**:
  ```
  IMPORT_PATH=/data/cores/cowbell/src
  USE_BOOLEAN_RETURNS=true
  CUSTOM_ERRORS=true
  USE_UUOBJIDS=true
  ANONYMOUS_OBJECTS=true
  ENABLE_EVENTLOG=true
  ```

#### Telnet Host Service
- **Image**: `tigwyk/moor-telnet-host:latest`
- **Ports**: 8888 (TCP)
- **Volume**: Mount `/data` as persistent volume
- **Environment Variables**:
  ```
  DAEMON_SERVICE_NAME=daemon
  DAEMON_RPC_PORT=7899
  DAEMON_EVENTS_PORT=7898
  DAEMON_ENROLLMENT_PORT=7900
  ```

#### Web Host Service
- **Image**: `tigwyk/moor-web-host:latest`
- **Ports**: 8081 (TCP)
- **Volume**: Mount `/data` as persistent volume
- **Environment Variables**:
  ```
  DAEMON_SERVICE_NAME=daemon
  DAEMON_RPC_PORT=7899
  DAEMON_EVENTS_PORT=7898
  DAEMON_ENROLLMENT_PORT=7900
  ```

#### Worker Service
- **Image**: `tigwyk/moor-worker:latest`
- **Volume**: Mount `/data` as persistent volume
- **Environment Variables**:
  ```
  DAEMON_SERVICE_NAME=daemon
  DAEMON_RPC_PORT=7899
  DAEMON_EVENTS_PORT=7898
  DAEMON_WORKERS_REQUEST_PORT=7896
  DAEMON_WORKERS_RESPONSE_PORT=7897
  DAEMON_ENROLLMENT_PORT=7900
  ```

#### Frontend Service
- **Image**: `tigwyk/moor-frontend:latest`
- **Ports**: 80 (HTTP)
- **Environment Variables**:
  ```
  WEB_HOST_SERVICE_NAME=webhost
  WEB_HOST_PORT=8081
  ```

## Service Discovery

Railway services in the same project can communicate using:
- `{service-name}.railway.internal` - Private DNS
- Environment variables auto-set by Railway

## First Run

On first deployment, the daemon will:
1. Fetch the Cowbell core from Codeberg
2. Generate encryption keys
3. Initialize the database
4. Start listening on TCP ports

Other services will enroll themselves via the enrollment port (7900).
