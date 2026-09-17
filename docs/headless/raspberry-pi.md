# Running a node on a Raspberry Pi

A Pi 5 is a good home for an always-on linXiv node: the server idles at
**~14 MiB RSS** on an empty library and near-zero CPU. The work is all in
getting there, because there is no published arm64 image yet.

## You build the image yourself

`ghcr.io/linxiv-dev/linxiv-headless` is **amd64 only** (see the
`publish-headless-image` job in `.github/workflows/release.yml`), so `podman
pull` on a Pi fails with "no matching manifest". The Pi compiles the Rust
backend from source instead — roughly 880 crates.

The `Dockerfile`'s cargo cache mounts are what make that survivable: they keep
the registry and `target/` across builds, so only the linXiv crates recompile
on an update. An incremental rebuild after a source change measures **2m50s**
on a 24-core x86 desktop; extrapolating, a Pi 5 should be in the 10-15 minute
range per update against 40-60 minutes for the first build. The incremental
cost is dominated by the release profile's `lto = true` + `codegen-units = 1`
link, which is largely serial, so the Pi's four cores do not help much there.

## What you need

- **Raspberry Pi 5, 8 GB.** Comfortable, and needs no swap. 4 GB builds too,
  with `CARGO_BUILD_JOBS=2` and a swapfile.
- **Active cooling.** A 40-minute build at full tilt will thermal-throttle a
  passively-cooled Pi 5 into roughly double that.
- **A 64-bit OS.** Not optional: `scripts/fetch_pdfium.sh` has no armhf asset,
  so a 32-bit userland fails before the build starts.
- **The linXiv repo on the Pi.** This doc assumes `~/Documents/linxiv`; adjust
  the paths below if yours is elsewhere.

## Setup

### 1. Flash the card and get a shell

Raspberry Pi Imager, **Raspberry Pi OS Lite (64-bit)**. Before writing, open
Imager's settings (the gear) and set the hostname, enable SSH, create your user
with an SSH key, and add Wi-Fi credentials if you are not on ethernet. Doing it
there saves attaching a monitor.

Boot the Pi, then from your laptop:

```bash
ssh <user>@<hostname>.local
uname -m        # must print aarch64
```

`armv7l` means a 32-bit image was flashed. Reflash before going further.

### 2. Update and install dependencies

```bash
sudo apt update && sudo apt full-upgrade -y
sudo apt install -y podman git curl openssl
podman --version
```

Podman must be **5.0 or newer**. The unit's `Health*` keys are what give the
node automatic recovery, and older podman silently ignores them. Pi OS Bookworm
ships 4.3, which is too old — use Pi OS Trixie, or install a newer podman.

### 3. Confirm rootless podman has a UID range

```bash
grep "^$USER:" /etc/subuid /etc/subgid
```

Both files should print a line. If either is missing, rootless containers
cannot start:

```bash
sudo usermod --add-subuids 524288-589823 --add-subgids 524288-589823 "$USER"
podman system migrate
```

### 4. Complete the checkout

The build needs the `crates/p2p` submodule — it is a workspace member and a
path dependency of the server, so an uninitialized one fails cargo immediately.
A plain `git clone` leaves it empty.

```bash
cd ~/Documents/linxiv
git submodule update --init --recursive
ls src-tauri/crates/p2p/Cargo.toml     # must exist
```

### 5. Create the data directory

Everything the node owns lives here: the database, PDFs, and the p2p identity.

```bash
sudo mkdir -p /mnt/linxiv/data
sudo chown -R "$USER" /mnt/linxiv
```

This is the path `linxiv.container` binds, so nothing in the unit needs editing.

### 6. Build the image

```bash
cd ~/Documents/linxiv
podman build -t linxiv-headless:local .
```

40-60 minutes the first time, and there is nothing to watch. Leave it.

### 7. Write the secrets file

Kept separate from the unit file, which is world-readable.

```bash
mkdir -p ~/.config/linxiv
cat > ~/.config/linxiv/node.env <<EOF
LINXIV_API_TOKEN=$(openssl rand -hex 32)
LINXIV_P2P_PASSPHRASE=$(openssl rand -hex 24)
EOF
chmod 600 ~/.config/linxiv/node.env
```

`LINXIV_P2P_PASSPHRASE` encrypts the p2p key store at rest, since there is no
OS keychain on a headless box. **Copy this file somewhere that is not the Pi.**
Losing it, or losing `/mnt/linxiv/data`, resets the node's identity and the
Node Address peers use to reach it.

### 8. Install the unit and start

```bash
mkdir -p ~/.config/containers/systemd
cp ~/Documents/linxiv/docs/headless/linxiv.container ~/.config/containers/systemd/

# Without linger, rootless user services stop when you log out and never start
# at boot. This is the step everyone forgets.
loginctl enable-linger "$USER"

systemctl --user daemon-reload
systemctl --user start linxiv
```

No `enable` step: Quadlet generates the unit with `WantedBy=default.target`
already wired at `daemon-reload`.

### 9. Verify

```bash
. ~/.config/linxiv/node.env
curl -sf -H "Authorization: Bearer $LINXIV_API_TOKEN" http://127.0.0.1:8000/api/status
podman healthcheck run linxiv && echo healthy
```

`/api/status` answering means migrations ran and the router is up. First boot
can take a minute; the unit allows 90s before health failures count.

### 10. Reboot and check it comes back

This is the real test of step 8, and the only way to find out now rather than
after a power cut.

```bash
sudo reboot
# wait, then from your laptop:
ssh <user>@<hostname>.local 'systemctl --user is-active linxiv'
```

## Reaching the node

The unit publishes on `127.0.0.1` only, so the node is reachable from the Pi
itself and nowhere else. From your laptop, tunnel over SSH:

```bash
ssh -N -L 8000:127.0.0.1:8000 <user>@<hostname>.local
```

Then `http://127.0.0.1:8000/admin` in a browser gives you the admin page: the
Remote Query Mode member list, the relay and PDF transfer logs, and the
copyable Node Address. It is served without auth, but every API call it makes
carries the bearer token, so it only works through the tunnel.

To put the node on the LAN instead, change `PublishPort` in
`linxiv.container` to `0.0.0.0:8000:8000`. The bearer token then becomes the
only thing between your library and the network.

## Keeping writes down

Worth doing on any Pi, and more so on a card:

- **Check `noatime` is on the root mount** — `findmnt -no OPTIONS /`. Pi OS
  normally sets it. Without it, every read turns into a write.
- **Leave the journal in RAM.** Debian only writes the systemd journal to disk
  if `/var/log/journal` exists. `ls -d /var/log/journal` — if it is absent,
  leave it absent, and the unit's `LogDriver=journald` costs nothing on disk.
- **Get a backup off the Pi** once the node is up:

  ```bash
  . ~/.config/linxiv/node.env
  curl -sf -X POST -H "Authorization: Bearer $LINXIV_API_TOKEN" \
    -H 'Content-Type: application/json' \
    -d '{"dest_path":"/data/backups/initial.db"}' \
    http://127.0.0.1:8000/api/storage/backup
  # then copy /mnt/linxiv/data/backups/initial.db off the Pi
  ```

One cold build moves far more data than weeks of the node running: about 1.7 GB
through the cargo cache mounts, plus the image layers. Updates are much cheaper,
but it is a reason not to rebuild idly.

## Health and restart

`linxiv.container` probes `GET /api/papers` every 30s, with a 90s start period
so first-boot migrations do not read as a failure.

The part worth understanding: **a restart policy alone does not cover the
failure you will actually get.** `Restart=always` reacts to process exit, but
an SBC's typical failure is a node that is alive and wedged — IO stall, thermal
throttle, an OOM-killed worker. Docker has no answer to that without a sidecar.
Podman does: `HealthOnFailure=kill` turns an unhealthy verdict into an exit,
which `Restart=always` then acts on. That pairing is why this doc uses Quadlet.

Verified against this unit by SIGSTOPping the node's main process (alive, not
answering): three failed probes, `status=137` from the kill, restart 10s later,
healthy again. About two minutes from wedge to recovery, unattended.

Add one probe from *outside* the Pi as well. The container healthcheck runs
inside the container and cannot see a broken port publish or a dead relay;
`GET /api/status` from another machine can.

```bash
systemctl --user status linxiv
journalctl --user -u linxiv -f
```

## Updating

```bash
~/Documents/linxiv/docs/headless/linxiv-update.sh
```

Snapshot, `git pull`, rebuild, restart, wait for healthy, prune. It backs up
first through the running node's own connection (`POST /api/storage/backup`)
rather than a second process, because the library is single-writer.

Watchtower and friends are useless here: there is no arm64 tag to poll.

**Run this manually, or on a timer that notifies rather than applies.** The
schema is pre-1.0 and still moving; an unattended migration on a box you do not
look at is how you find out three weeks late. If you want it scheduled anyway:

```ini
# ~/.config/systemd/user/linxiv-update.timer
[Unit]
Description=Weekly linXiv node update
[Timer]
OnCalendar=Sun 04:00
Persistent=true
[Install]
WantedBy=timers.target
```

with a matching `linxiv-update.service` running the script, then
`systemctl --user enable --now linxiv-update.timer`.

The cargo cache mounts cost about 1.7 GB (395 MB registry, 1.28 GB `target/`,
measured after a build). Rootless podman keeps them in
`/var/tmp/buildah-cache-$(id -u)/`, which `podman builder prune` does **not**
touch — it only prunes images. Leave them alone unless the disk is actually
tight: deleting that directory buys back the space at the cost of making the
next update a full cold build.

## Troubleshooting

| Symptom | Cause |
|---|---|
| `podman pull` says "no matching manifest" | Expected. There is no arm64 image; build it. |
| Panics at startup with `unable to open database file: /data/papers.db` | The container cannot write the bind mount. On an SELinux host, the `:Z` on the `Volume=` line is missing. Otherwise check `/mnt/linxiv/data` exists and you own it. |
| `unsupported host Linux-armv7l` from `fetch_pdfium.sh` | 32-bit userland. Reflash with a 64-bit image. |
| Unit works when you are logged in, node is gone after a reboot | `loginctl enable-linger "$USER"` was not run. |
| Container restart-loops with a healthy-looking log | The healthcheck is failing, most likely a wrong or missing `LINXIV_API_TOKEN` in `node.env`. `podman inspect linxiv --format '{{json .State.Health.Log}}'` shows the probe output. |
| Build is killed partway through | Out of RAM. `CARGO_BUILD_JOBS=2` and add a swapfile. |
| The build fails early with a cargo error naming `crates/p2p` | The submodule is empty. `git submodule update --init --recursive`. |
| Node is unhealthy but never restarts | podman is older than 5.0 and ignored the `Health*` keys. `podman --version`. |
| `podman run` fails with a subuid/subgid error | No UID range for your user; see setup step 3. |

## How small can you go?

At runtime, far smaller than you would think: ~14 MiB idle, a 50 MB image, one
mostly-sleeping process. A 2 GB Pi 5 would serve a library fine. Even a Pi Zero
2 W (arm64, 512 MB) would *run* it, though PDF text extraction would crawl.

The constraint is entirely the build. If you want to run on something small,
cross-compile `aarch64-unknown-linux-gnu` on a desktop with `cargo zigbuild` or
`cross` and ship a thin image that just `COPY`s the binary and `libpdfium.so` —
the runtime stage of the existing `Dockerfile` is already almost exactly that.

The other direction is to fix it upstream: GitHub now offers free
`ubuntu-24.04-arm` runners for public repos, so `publish-headless-image` could
matrix over amd64 and arm64 and merge a manifest. Then the Pi pulls in seconds
and none of the build section above matters.

## Tuning

The release profile (`lto`, `codegen-units = 1`, `opt-level = "s"`, `strip`) is
already tuned; there is nothing left to switch on. `opt-level = 3` with
`-C target-cpu=cortex-a76` is worth *measuring* but not worth assuming — `"s"`
often wins on a cache-starved A76.

Two knobs that do matter on a Pi:

| Setting | Where | Why |
|---|---|---|
| `PodmanArgs=--memory=1500m` | `linxiv.container` | Keeps one pathological PDF's extraction spike from OOMing the whole Pi. |
| `LINXIV_PDF_RATE_BPS` | `node.env` | Throttles the Remote Query PDF lane when the uplink, not the Pi, is the bottleneck. |

If it feels slow under real use, watch `podman stats` and `vmstat 5` rather
than guessing. The bottleneck will be disk IO on the SQLite and PDF path
rather than the CPU, so a compiler flag is not what fixes it.
