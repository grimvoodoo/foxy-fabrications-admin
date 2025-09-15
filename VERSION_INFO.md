# Admin Version Info System

This repository includes a version info system to help identify which container image is currently running.

## Endpoints

### `/info` - Version Information
Returns detailed information about the running admin service:

```json
{
  "service": "foxy-fabrications-admin",
  "image": "ghcr.io/acleveland/foxy-fabrications-admin:v1.2.3",
  "build_time": "2025-01-15T22:30:45Z",
  "git_commit": "abc123def456789...",
  "environment": "production"
}
```

### `/health` - Health Check
Returns basic health status:

```json
{
  "status": "healthy",
  "service": "foxy-fabrications-admin"
}
```

## Usage

### Check Version of Running Admin Service
```bash
# For admin site (running on port 3001) 
curl http://localhost:3001/info
curl http://localhost:3001/health
```

### Identify Container Image
The `image` field tells you exactly which container image is running:
- `ghcr.io/acleveland/foxy-fabrications-admin:v1.2.3` - Specific version
- `ghcr.io/acleveland/foxy-fabrications-admin:latest` - Latest version
- `local-dev-admin` - Development build

## How It Works

1. **Build Time**: During CI/CD, GitHub Actions writes version info to `version.txt`
2. **Container Build**: The Containerfile copies `version.txt` into the image
3. **Runtime**: The `/info` endpoint reads `version.txt` and returns the data

## Development

For local development, the `version.txt` file contains `local-dev-admin,unknown,unknown` indicating it's a development build.

## Troubleshooting

If you see:
- `"image": "unknown"` - The version.txt file is missing or unreadable
- `"image": "local-dev-admin"` - You're running a development build
- Specific image name - You're running a deployed container

This helps identify if you're testing against an old deployed admin version vs. your latest code changes.

## Product Image Path Issue

Use this endpoint to verify which version of the admin service is running when troubleshooting the product image path issue:

- If admin shows old image name: Old admin version is deployed, needs restart with new code
- If admin shows `local-dev-admin`: You're testing local development code 
- If admin shows current image name: Latest admin version is running