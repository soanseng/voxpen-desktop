# VoxPen Desktop — Release Process

## Repository Architecture

| Repo | Visibility | Purpose |
|------|-----------|---------|
| `soanseng/voxpen-desktop` | Private | Source code, CI/CD workflows |
| `soanseng/voxpen-releases` | Public | Release binaries, `latest.json` for auto-update |

Users only see `voxpen-releases`. Source code stays private.

## How a Release Works

```
git tag v0.6.0 && git push origin v0.6.0
         │
         ▼
┌─────────────────────────────────────────┐
│  .github/workflows/release.yml          │
│  (runs in soanseng/voxpen-desktop)      │
└──────────────┬──────────────────────────┘
               │
    ┌──────────▼──────────┐
    │  build (3 parallel)  │
    │  • Windows x64       │  → .exe + .exe.sig
    │  • macOS arm64       │  → .dmg + .dmg.sig
    │  • Linux x64         │  → .AppImage + .AppImage.sig + .deb
    └──────────┬──────────┘
               │ upload-artifact
    ┌──────────▼──────────┐
    │  publish job         │
    │  • Download all      │
    │  • Generate latest.json
    │  • Create release on │
    │    voxpen-releases   │
    └──────────┬──────────┘
               │
    ┌──────────▼──────────────────────┐
    │  soanseng/voxpen-releases       │
    │  Release: v0.6.0                │
    │  Assets:                        │
    │   ├── VoxPen Desktop_x64-setup.exe
    │   ├── VoxPen Desktop_x64-setup.exe.sig
    │   ├── VoxPen Desktop_aarch64.dmg
    │   ├── VoxPen Desktop_aarch64.dmg.sig
    │   ├── VoxPen Desktop_amd64.AppImage
    │   ├── VoxPen Desktop_amd64.AppImage.sig
    │   ├── VoxPen Desktop_amd64.deb
    │   └── latest.json               │
    └─────────────────────────────────┘
```

## Step-by-Step: Creating a Release

### 1. Bump version

Update version in both files (must match):

```bash
# src-tauri/tauri.conf.json → "version": "0.6.0"
# src-tauri/Cargo.toml      → version = "0.6.0"
```

### 2. Commit and tag

```bash
git add -A
git commit -m "chore: bump version to 0.6.0"
git tag v0.6.0
git push origin main --tags
```

### 3. Wait for CI

The `release.yml` workflow triggers on `v*` tags. Monitor at:
- https://github.com/soanseng/voxpen-desktop/actions

### 4. Verify release

Check the published release at:
- https://github.com/soanseng/voxpen-releases/releases

Verify `latest.json` is accessible:
```bash
curl -sL https://github.com/soanseng/voxpen-releases/releases/latest/download/latest.json | jq .
```

## Auto-Update (Tauri Updater)

### How it works

1. Running app periodically fetches `latest.json` from `voxpen-releases`
2. Compares `version` field against current app version
3. If newer: downloads platform-specific binary
4. Verifies Ed25519 signature against embedded public key
5. Installs and restarts

### Configuration

**`tauri.conf.json`**:
```json
{
  "bundle": {
    "createUpdaterArtifacts": true
  },
  "plugins": {
    "updater": {
      "endpoints": [
        "https://github.com/soanseng/voxpen-releases/releases/latest/download/latest.json"
      ],
      "pubkey": "<public-key>"
    }
  }
}
```

### `latest.json` format

Generated automatically by the publish job:

```json
{
  "version": "0.6.0",
  "notes": "VoxPen Desktop v0.6.0",
  "pub_date": "2026-02-27T12:00:00Z",
  "platforms": {
    "windows-x86_64": {
      "signature": "<base64-sig>",
      "url": "https://github.com/soanseng/voxpen-releases/releases/download/v0.6.0/VoxPen+Desktop_0.6.0_x64-setup.exe"
    },
    "darwin-aarch64": {
      "signature": "<base64-sig>",
      "url": "https://github.com/soanseng/voxpen-releases/releases/download/v0.6.0/VoxPen+Desktop_0.6.0_aarch64.dmg"
    },
    "linux-x86_64": {
      "signature": "<base64-sig>",
      "url": "https://github.com/soanseng/voxpen-releases/releases/download/v0.6.0/VoxPen+Desktop_0.6.0_amd64.AppImage"
    }
  }
}
```

## Signing Keys

### Updater signing keypair

Generated with `cargo tauri signer generate`. Used to sign update artifacts so the app can verify authenticity before installing.

| Item | Location |
|------|----------|
| Private key | `src-tauri/.tauri/keys` (gitignored, never commit) |
| Public key | Embedded in `tauri.conf.json` → `plugins.updater.pubkey` |
| CI secret | `TAURI_SIGNING_PRIVATE_KEY` on `soanseng/voxpen-desktop` |

**If you lose the private key**, you must generate a new pair and push a manual update (users cannot auto-update across key changes).

### Regenerating keys

```bash
cargo tauri signer generate --password "" -w src-tauri/.tauri/keys

# Update pubkey in tauri.conf.json
cat src-tauri/.tauri/keys.pub

# Update CI secret
gh secret set TAURI_SIGNING_PRIVATE_KEY --repo soanseng/voxpen-desktop < src-tauri/.tauri/keys
```

## GitHub Secrets

Required secrets on `soanseng/voxpen-desktop`:

| Secret | Purpose | How to create |
|--------|---------|---------------|
| `TAURI_SIGNING_PRIVATE_KEY` | Signs updater artifacts (.sig files) | `cargo tauri signer generate` |
| `RELEASE_PAT` | Push releases to `voxpen-releases` | Fine-grained PAT with `contents:write` on `voxpen-releases` |

### Creating RELEASE_PAT

1. Go to https://github.com/settings/tokens?type=beta
2. **Token name**: `voxpen-release-publisher`
3. **Repository access**: Only select `soanseng/voxpen-releases`
4. **Permissions**: Contents → Read and write
5. Generate and copy the token
6. Set as secret:
   ```bash
   gh secret set RELEASE_PAT --repo soanseng/voxpen-desktop
   ```

### Optional: macOS code signing secrets

| Secret | Purpose |
|--------|---------|
| `APPLE_CERTIFICATE` | Base64-encoded .p12 Developer ID certificate |
| `APPLE_CERTIFICATE_PASSWORD` | .p12 export password |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Name (TEAMID)` |
| `APPLE_TEAM_ID` | 10-char Apple team ID |
| `APPLE_ID` | Apple ID email |
| `APPLE_PASSWORD` | App-specific password from appleid.apple.com |

## Troubleshooting

### Release workflow fails at "publish" step

- Check that `RELEASE_PAT` secret is set and has `contents:write` on `voxpen-releases`
- Fine-grained PATs expire — regenerate if older than the configured lifetime

### `latest.json` returns 404

- Ensure the release on `voxpen-releases` is **not** a draft
- Check the tag name matches (e.g. `v0.6.0`)
- GitHub CDN may take a few minutes to propagate

### Updater says "up to date" but new version exists

- Verify `tauri.conf.json` version was bumped before tagging
- Check that `latest.json` version is strictly greater than the installed version
- Confirm the endpoint URL is correct in `tauri.conf.json`

### Signature verification failed

- The signing key in CI must match the public key in `tauri.conf.json`
- If keys were regenerated, users on old versions cannot auto-update (must download manually)

### Build artifacts missing for a platform

- Check the matrix job logs in GitHub Actions
- Platform-specific failures (e.g. missing system deps) don't fail other platforms (`fail-fast: false`)
