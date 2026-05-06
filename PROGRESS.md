# IRL — Progress & Pending

## Completed — 2026-05-06

### Technical
- WASM engine (Rust) running in demo.html and spec.html
- `runIrlEval()` naming fix — avoids `document.evaluate()` DOM collision
- Solana Pay QR embedded as base64 in demo.html and spec.html (no CDN dependency)
- Solana QR as PNG (black on white) in README for GitHub light theme
- Wallet address: `NC3zNzcx9gDMYWB2AQDTptbA66DJ4oWd7RBHgaJvEMC`
- GitHub avatar as favicon in demo.html and spec.html

### Distribution
| Channel | Link | Date |
|---|---|---|
| LinkedIn post | [post](https://www.linkedin.com/feed/update/urn:li:activity:7456438897800052736/) | 2026-05-06 |
| LinkedIn comment | MCP discussion referenced in comments | 2026-05-06 |
| MCP discussion | [#2693](https://github.com/modelcontextprotocol/modelcontextprotocol/discussions/2693) | 2026-05-06 |
| Railway issue | [railway-skills #36](https://github.com/railwayapp/railway-skills/issues/36) | 2026-05-06 |
| X / Twitter | [@IchasoRodrigo](https://x.com/IchasoRodrigo/status/2052122640068808853) | 2026-05-06 |

---

## Pending — Next Session

### Distribution
- [ ] Monitor MCP discussion #2693 — reply fast if someone comments
- [ ] Monitor Railway issue #36 — reply fast if someone comments
- [ ] Hacker News (Show HN) — wait for some GitHub traction first (stars, comments)
- [ ] Full X thread (4 tweets) once there's an audience
- [ ] Second tweet when MCP or Railway responds — use it to amplify

### Project
- [ ] Build karma on Hacker News before Show HN post
- [ ] X banner with IRL logo
- [ ] Think about v0.2 scope based on community feedback
- [ ] Real integration proof of concept: IRL + railway-skills

### Unrelated (same server)
- [ ] Recover D2Restaurant from NVMe
  - Backup ready at `/mnt/nvme_tmp/Test/Web/d2restaurant_backup_20260426.sql` on Proxmox host (192.168.0.4)
  - MariaDB, DB name: `d2restaurant`, app runs on port 8888
  - Also recover: `wwwroot/`, `Config.ini`, `DLL/libmariadb.dll`
  - Unmount NVMe after recovery: `umount /mnt/nvme_tmp` on host
