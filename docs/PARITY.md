# Rodon feature map

The reference is evanovar’s Python source at `8b37709`. This table connects upstream behavior to the Rust code that implements it. The last section describes differences that matter when migrating or testing. [Upstream snapshot](https://github.com/evanovar/RobloxAccountManager/tree/8b37709f7b47528a2a481082d743cb681174b003)

| Upstream area | Rodon implementation |
| --- | --- |
| Account manager, encryption, operation results | `src/store`, `src/core/accounts.rs` and `src/error.rs` implement vault locking, atomic saves, recovery and Windows or passphrase protection. |
| Account backup and migration | `src/import` reads portable backups and earlier managers’ formats. `src/core/accounts.rs` implements merging, overwrite choices and per-account changes. |
| Account order, filters, groups, notes, favorites and recents | `src/ui/pages/accounts.rs`, `src/ui/games.rs` and `src/store/model.rs` handle account selection, ordering, grouping and saved launch destinations. |
| Browsers, Chromium and account creator | `src/browser`, `src/ui/pages/add_account.rs` and `src/ui/signup.rs` implement browser selection, portable Chromium, Quick Sign-in and registration drafts. |
| Roblox API, cookie validation, presence, avatars and subscription status | `src/roblox` and `src/services` resolve account details, check sessions, fetch game presence and cache avatars and subscription badges. |
| Account actions and launch monitor | `src/core/launcher.rs`, `src/roblox/launch.rs` and `src/roblox/logs.rs` implement launch targets, launcher choice, tracker matching and pause-on-mismatch behavior. |
| Game selector and private servers | `src/ui/games.rs`, `src/ui/pages/servers.rs` and `src/ui/pages/private_servers.rs` handle discovery, public server pages, server history, owned servers and link renewal. |
| Multi Roblox, headless windows, process and window operations | `src/win` and `src/services/instances.rs` manage instance coordination, visibility, process controls and memory trimming. Singleton handle support uses Windows APIs. |
| Auto-Rejoin and Anti-AFK | `src/services/auto_rejoin.rs` and `src/services/anti_afk.rs` handle per-account destinations, retry limits, connection checks and scheduled input. |
| Window grid, log probe, renamer and startup | `src/win/grid.rs`, `src/win/hotkey.rs`, `src/services/activity.rs`, `src/services/renamer.rs` and `src/win/startup.rs` handle layout, global hotkeys, client identification, titles and shortcuts. |
| Player downloader, Roblox settings and installer handling | `src/services/downloader.rs`, `src/services/roblox_settings.rs`, `src/services/installers.rs` and `src/ui/pages/roblox_settings.rs` handle packages, staged XML edits, backups and reversible installer moves. |
| Color themes, theme library, media, styles and editor | `src/themes.rs`, `assets/themes.json` and `src/ui/{theme,theme_editor,background,video}.rs` implement the eight upstream presets, derived colors, named copies, portable themes and media decoding. |
| Desktop pages, icons, motion, splash, Console and Donations | `src/ui` implements the native sidebar, detached viewports, tray menu, splash, transitions, log viewer and links to evanovar’s donation page. |
| Diagnostics, app paths, settings, atomic I/O and updates | `src/logging.rs`, `src/paths.rs`, `src/store` and `src/services/updater.rs` handle local diagnostics, profile locations, persistence and checksummed updates from Rodon releases. |
| WebSocket server | `src/api/websocket.rs` handles Ping, AccountList, GetStatus, Launch, JoinUser, AutoRejoin and locally reviewed Add requests. `src/api/http.rs` and `src/api/mcp.rs` expose the Rodon command set. |

## Migration and verification

- Browser sign-in requires approval on Roblox. The two agreed exclusions are bulk username/password login attempts and execution of supplied JavaScript on the Roblox login page. Registration drafts must be completed individually on the Roblox website.
- Use a portable password backup to transfer an Evanovar hardware-bound vault. Rodon writes its own encrypted backup format and also reads Evanovar backups.
- New profiles reserve port `7963` for WebSocket clients and `7964` for HTTP. Both listeners are opt-in. Existing profiles keep their configured ports. A WebSocket `Add` request requires review in the open desktop app.
- Windows Media Foundation supplies video codecs. Format support depends on the codecs installed on the machine.
- The theme suite imports a background and custom colors from an archive produced by the original Python exporter. Unit and integration tests also cover local behavior with synthetic account data. The Windows GUI smoke test visits each page and a detached console. Signed-in launches, private servers, multiple active Roblox accounts and extended gameplay need separate live verification.
