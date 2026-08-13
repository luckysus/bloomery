# Signed Updates / 签名更新

Bloomery checks for updates only when the user presses the update button in
Settings. The Tauri updater verifies the release signature before download and
installation. The application never executes an unsigned update.

Bloomery 只会在用户进入设置并主动点击检查按钮时检查更新。Tauri updater
会在下载和安装前验证发行签名，未签名更新不会被执行。

## Release configuration

The repository configuration contains the public GitHub update endpoint but no
trust key. Engineering builds therefore remain unsigned. A signed build must
provide all of these values through the protected release environment:

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = "<private key supplied by the signing system>"
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "<optional password>"
$env:BLOOMERY_OFFICIAL_PRIVATE_KEY_2026 = "<64 hexadecimal characters for the 32-byte Ed25519 seed>"
$env:BLOOMERY_OFFICIAL_PUBLIC_KEY_2026 = "<matching 64-hex public key>"
$env:BLOOMERY_UPDATER_PUBLIC_KEY = "<matching public key>"
$env:BLOOMERY_UPDATER_ENDPOINT = "https://github.com/luckysus/bloomery/releases/latest/download/latest.json"
$env:BLOOMERY_RELEASE_ASSET_BASE_URL = "https://github.com/luckysus/bloomery/releases/download/<tag>"
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-release.ps1 -Signed -Bundles nsis
```

`scripts/write-updater-config.ps1` creates a temporary Tauri configuration
overlay. It rejects missing keys, non-HTTPS endpoints, and local/private hosts.
The overlay and private key are never committed or copied into the installer.

The public key and its rotation policy must be published with the release
documentation. Keep the private key in the approved signing system, not in a
developer profile, repository secret dump, or build artifact.

`BLOOMERY_OFFICIAL_PRIVATE_KEY_2026` is consumed only by the temporary Rust
release signer. It creates the official steel package `signature.json`, checks
that the derived public key matches `BLOOMERY_OFFICIAL_PUBLIC_KEY_2026`, and is
never written to the package, installer, logs, or repository.

## Update metadata

Every public release must publish a Tauri-compatible `latest.json` beside the
signed Windows updater artifact. With the current Tauri 2 configuration, the
NSIS updater artifact is `*-setup.exe` with a matching `*.exe.sig`; legacy
`*.nsis.zip` and `*.msi.zip` artifacts are also accepted by the manifest
generator when the corresponding current installer is absent. The generated
manifest contains `windows-x86_64-nsis` for installed NSIS clients,
`windows-x86_64-msi` for installed MSI clients, and the generic
`windows-x86_64` fallback for portable clients. The portable fallback points
to the signed NSIS installer. The metadata, installer signatures, checksum,
SBOM, and notices are one release set and must be verified together.
