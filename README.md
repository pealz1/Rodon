# Rodon

Rodon is a native Windows account manager for Roblox, written in Rust. Use it to organize accounts, find games and private servers, and manage running Roblox windows.

## Credits

**evanovar** created the Python Roblox Account Manager that Rodon forks. **ic3w0lf22** created the earlier Roblox Account Manager that inspired it. Rodon builds on the existing Rust rewrite in **pealz1**’s fork. Source and derivative work are distributed under GPL-3.0-only. [Evanovar RAM](https://github.com/evanovar/RobloxAccountManager) · [Original Roblox Account Manager](https://github.com/ic3w0lf22/Roblox-Account-Manager) · [License](LICENSE)

## Run on Windows

The Windows CI run for each Rust commit builds an artifact named `Rodon-Windows-x64`. Open a successful run, download its artifact, extract `Rodon.exe` into a writable folder and run it. [Windows builds](https://github.com/pealz1/Rodon/actions/workflows/ci.yml?query=branch%3Arodon-rust)

To build locally, use the Windows x64 MSVC Rust toolchain (1.95 or newer), the Visual Studio C++ build tools and a Windows SDK.

```powershell
git clone https://github.com/pealz1/Rodon.git
cd Rodon
cargo build --release --locked
.\target\release\Rodon.exe
```

On first launch, choose **Accounts → Add account**. Start Quick Sign-in and approve the displayed code in the official Roblox sign-in flow for your account. Registration opens on Roblox and is completed in the browser. [Roblox Quick Login instructions](https://en.help.roblox.com/hc/en-us/articles/360056582012-Quick-Login)

## What you can do

| Area | Features |
| --- | --- |
| Accounts | Search, filter, group, reorder, star, edit notes and aliases, check session status, and import or export encrypted account backups. |
| Games and servers | Search or browse games, save favorites, inspect public servers, join by place or job ID, follow a joinable user, review join history and manage private-server links. |
| Running clients | Launch multiple accounts, inspect CPU and memory, arrange or hide windows, set a grid hotkey, configure Auto-Rejoin and schedule Anti-AFK input. |
| Roblox settings | Stage and apply XML settings, manage launcher preferences, download a Roblox Player build, and install portable Chromium for browser approval. |
| Desktop | Detach pages, view logs in the Console, use the tray, configure Windows startup, and import or export `.ramtheme` files with a still image, GIF or video background. |
| Local integrations | Use the optional loopback HTTP and WebSocket services or run the MCP server over stdio. |

The feature map records implementation locations, upstream file compatibility and verification limits. [Feature map](docs/PARITY.md) · [Local API guide](docs/API.md)

## Profiles and migration

By default, Rodon writes its vault, settings, logs and themes to `RodonData` beside the executable. New vaults use Windows account protection. Select password protection to unlock the vault with a passphrase. Use **Settings** to write a password-protected portable backup.

To migrate an Evanovar hardware-encrypted profile, export a password-protected account backup from Evanovar RAM on the original PC. Import that file in Rodon with its backup password. Rodon can also read supported `saved_accounts.json` and ic3w0lf22 `AccountData.json` files that you choose in the import dialog.

| Option | Effect |
| --- | --- |
| `--data-dir "D:\Profiles\Rodon"` or `RODON_DATA_DIR` | Use a separate profile. The command-line option takes precedence. |
| `--mcp` | Run the local MCP server over standard input and output. |
| `--smoke-test` | Render each navigation page and a detached console in a temporary profile, then exit. |
| `--version` | Print the version and exit. |

## Development checks

```powershell
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all
cargo build --release --locked
Start-Process .\target\release\Rodon.exe -ArgumentList '--smoke-test' -Wait -PassThru
```
