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

## Hardware and OS

- **64-bit OS, mandatory.** `scripts/fetch_pdfium.sh` has no armhf asset; a
  32-bit userland fails at the fetch step. Raspberry Pi OS Lite (64-bit) or
  Ubuntu Server arm64.
- **Pi 5, 8 GB** is comfortable. 4 GB builds too, with `CARGO_BUILD_JOBS=2`
  and swap on the SSD. The build is what wants RAM; see "How small" below for
  the runtime side.
- **Active cooling.** A 40-minute build at full tilt will thermal-throttle a
  passively-cooled Pi 5 into roughly double that.
- **podman >= 5.0**, for the `Health*` Quadlet keys in `linxiv.container`.
  Check with `podman --version`; Pi OS Bookworm's podman 4.3 is too old.

## Storage

Put `/data` on an SSD, not the SD card. The node's write pattern is SQLite in
WAL mode plus PDF blobs — sustained small writes, which is precisely what wears
SD cards out and corrupts databases. This is the one item here that is about
not losing your library.

Either works:

- **NVMe via a Pi 5 M.2 HAT** (~450-900 MB/s). Best, costs money.
- **A 2.5" SATA SSD on a UASP-capable USB 3.0 adapter** (~300-400 MB/s). Free
  if you have the drive. Insist on UASP — non-UASP bridges use BOT, which is
  slow and drops the disk under sustained load.

Either way, mount it and hand it to your user:

```bash
sudo mkdir -p /mnt/linxiv-ssd
# find the UUID, then add to /etc/fstab so it mounts at boot:
lsblk -o NAME,SIZE,UUID,MOUNTPOINT
echo 'UUID=<uuid> /mnt/linxiv-ssd ext4 defaults,noatime 0 2' | sudo tee -a /etc/fstab
sudo mount -a
sudo mkdir -p /mnt/linxiv-ssd/data && sudo chown -R "$USER" /mnt/linxiv-ssd
```

`noatime` is there to stop every read from turning into a write.

## Install

```bash
sudo apt install -y podman git
git clone --recurse-submodules https://github.com/linxiv-dev/linXiv.git ~/linXiv
podman build -t linxiv-headless:local ~/linXiv        # go do something else

# Secrets, separate from the world-readable unit file.
mkdir -p ~/.config/linxiv
cat > ~/.config/linxiv/node.env <<EOF
LINXIV_API_TOKEN=$(openssl rand -hex 32)
LINXIV_P2P_PASSPHRASE=$(openssl rand -hex 24)
EOF
chmod 600 ~/.config/linxiv/node.env

# The Quadlet unit next to this file.
mkdir -p ~/.config/containers/systemd
cp ~/linXiv/docs/headless/linxiv.container ~/.config/containers/systemd/

# Without linger, rootless user services stop when you log out and never
# start at boot. This is the step everyone forgets.
loginctl enable-linger "$USER"

systemctl --user daemon-reload
systemctl --user start linxiv
```

Verify:

```bash
. ~/.config/linxiv/node.env
curl -sf -H "Authorization: Bearer $LINXIV_API_TOKEN" http://127.0.0.1:8000/api/status
podman healthcheck run linxiv && echo healthy
```

`LINXIV_P2P_PASSPHRASE` encrypts the p2p key store at rest, since there is no
OS keychain here. Losing it, or losing `/mnt/linxiv-ssd/data`, resets the
node's identity and the Node Address peers use to reach it. Back up
`node.env` somewhere off the Pi.

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
~/linXiv/docs/headless/linxiv-update.sh
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
| Panics at startup with `unable to open database file: /data/papers.db` | The container cannot write the bind mount. On an SELinux host, the `:Z` on the `Volume=` line is missing. Otherwise check `/mnt/linxiv-ssd/data` exists and you own it. |
| `unsupported host Linux-armv7l` from `fetch_pdfium.sh` | 32-bit userland. Reflash with a 64-bit image. |
| Unit works when you are logged in, node is gone after a reboot | `loginctl enable-linger "$USER"` was not run. |
| Container restart-loops with a healthy-looking log | The healthcheck is failing, most likely a wrong or missing `LINXIV_API_TOKEN` in `node.env`. `podman inspect linxiv --format '{{json .State.Health.Log}}'` shows the probe output. |
| Build is killed partway through | Out of RAM. `CARGO_BUILD_JOBS=2` and add swap on the SSD. |

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
than guessing. The bottleneck will be disk IO on the SQLite and PDF path, and
the fix for that is the SSD, not a compiler flag.
