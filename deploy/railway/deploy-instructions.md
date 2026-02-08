# Deploying mooR to Railway

## Option 1: Via Railway Web Dashboard (Recommended for WSL)

Since WSL has an old Node.js version, the easiest deployment method is via the Railway web interface:

### Step 1: Push to GitHub

```bash
git add .
git commit -m "feat: Add Railway deployment configuration"
git push
```

### Step 2: Create Railway Project

1. Go to https://railway.app/new
2. Click "Deploy from GitHub repo"
3. Select your `moor` repository
4. Railway will detect the `railway.toml` configuration

### Step 3: Configure Persistent Volume

1. In your Railway project, go to **Settings** > **Volumes**
2. Click **Create Volume**
3. Name: `data`
4. Mount Path: `/data`

### Step 4: Configure Environment Variables (if needed)

The `railway.toml` includes defaults, but you can override in **Variables** tab:

| Variable | Value |
|----------|-------|
| `PORT` | `8080` |
| `RUN_DIR` | `run-cowbell` |
| `MCP_CREATION_POLICY` | `open` |

### Step 5: Deploy

Click **Deploy** or push a new commit to trigger automatic deployment.

### Step 6: Access Your App

After deployment, Railway will provide a URL like:
- `https://your-project.up.railway.app`

## Option 2: Fix Node.js in WSL

If you want to use the Railway CLI from WSL:

```bash
# Install Node.js 20+ via nvm
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash
source ~/.bashrc
nvm install 20
nvm use 20

# Reinstall Railway CLI
npm uninstall -g @railway/cli
npm install -g @railway/cli

# Then proceed with deployment
railway login
railway init
railway volume create data
railway up
```

## Option 3: From Windows PowerShell

Open PowerShell (not WSL) and run:

```powershell
# Navigate to your project (using Windows path)
cd C:\dev\moor  # or wherever your repo is

# Login and initialize
railway login
railway init

# Create persistent volume
railway volume create data

# Deploy
railway up
```

## Verification

After deployment, verify:
- Health check: `https://your-project.up.railway.app/health`
- Web UI: `https://your-project.up.railway.app`
- Logs available in Railway dashboard
