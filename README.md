# ironclaw-forge

Pre-built Docker image of [IronClaw](https://github.com/nearai/ironclaw) for Agent Forge deployment on Railway.

## Image

```
ghcr.io/mrderivatives/ironclaw-forge:latest
ghcr.io/mrderivatives/ironclaw-forge:v0.18.0
```

## Build

Trigger via GitHub Actions → "Build IronClaw Docker Image" workflow.

- Platform: `linux/amd64` (Railway)
- Base: IronClaw's own Dockerfile (multi-stage Rust build)
