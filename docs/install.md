# Install

There are no published binaries or releases yet. Build from source (needs Go) and copy the result:

```
make cross        # bin/protectorate-{linux,windows}-{amd64,arm64}[.exe], bin/protectorate-android-arm64
make build        # bin/protectorate for this machine
```

`protectorate service ...` only prints or writes service definitions. It never enables or starts anything, and `--write DIR` / `--install` refuse to overwrite existing files. Arguments after `--` go to `hub serve` / `node serve`.

## Hub (Linux)

```
install -m755 bin/protectorate-linux-amd64 ~/.local/bin/protectorate
protectorate hub tailnet install
protectorate hub tailnet init --server-url http://LAN-IP:7480        # or an https URL
protectorate service systemd hub --binary ~/.local/bin/protectorate --install -- --web ~/protectorate/app/build/web
systemctl --user daemon-reload && systemctl --user enable --now protectorate-hub
sudo loginctl enable-linger $USER                                    # keep running when logged out
```

The unit uses `KillMode=control-group` so a killed hub cannot orphan headscale. Prefer the user unit when the hub runs sleeves (they use `systemd-run --user`). For a headless box without a user session, `--user-name NAME` renders a system unit for `/etc/systemd/system`.

## Nodes

On the hub: `protectorate node invite <name>`. It prints a `node join ...` line; run that on the node, then run `node serve` as a service.

Linux:

```
protectorate node join --control URL --key KEY --name NAME
protectorate service systemd node --binary ~/.local/bin/protectorate --install
systemctl --user daemon-reload && systemctl --user enable --now protectorate-node
```

Windows (x86-64: `protectorate-windows-amd64.exe`, ARM: `protectorate-windows-arm64.exe`). `node serve` runs as a real Windows service, with no wrapper. Use one `PROTECTORATE_HOME` for both `node join` and the service:

```
protectorate.exe node join --control URL --key KEY --name NAME
protectorate.exe service install-windows --home C:\ProgramData\protectorate > install.cmd
REM review install.cmd, then run it from an elevated prompt
```

A process started over SSH keeps running after the session closes, so do not use that for the long-running agent. Alternative without service rights: `protectorate.exe service windows-task --trigger logon --write .` then `schtasks /Create /TN protectorate-node /XML protectorate-node.xml`.

### Android (Termux)

`protectorate-android-arm64` is a static binary; copy it into Termux and run it as the Termux user (not root). Android runs no sleeves, so a phone is a build/test node: hub jobs, `job test-cross`, metrics.

```
# in Termux
cp /sdcard/Download/protectorate-android-arm64 $PREFIX/bin/protectorate && chmod +x $PREFIX/bin/protectorate
pkg install git                     # needed for git-source jobs; test-cross works without it
# on the hub
protectorate node invite phone      # prints the join line; the control URL must be reachable from the phone
# in Termux
protectorate node join --control URL --key KEY --name phone
pkg install termux-services         # then restart Termux
protectorate service termux --install && sv-enable protectorate-node
# on the hub
protectorate node show phone         # invite already recorded it in the inventory
```

Without `--name`, the node name comes from the device model (`ro.product.model`) because Android reports the hostname `localhost`. Install the Termux:Boot app and open it once so the boot script runs after a reboot; the generated script takes a wake lock. `tmux` (`pkg install tmux`) is only needed to run the session tests there.

Verified in `termux/termux-docker:aarch64` (a normal Linux kernel with the Termux rootfs and the Android `/system/bin/sh`), as the container's `system` user. The container runs on the hub host with `--network host`, because the hub's own tailnet connection to the docker bridge address (172.17.0.1) left via the LAN interface and never connected. The hub used `--server-url http://127.0.0.1:PORT` and loopback listen/DERP/STUN addresses:

```
docker run -d --name test-prot-android-1 --network host \
  -v $PWD/bin/protectorate-android-arm64:/data/local/tmp/protectorate:ro \
  -v $STATE:/data/data/com.termux/files/home/state \
  termux/termux-docker:aarch64 /system/bin/sh -c 'sleep 100000'
docker exec -u system -e HOME=/data/data/com.termux/files/home -e PREFIX=/data/data/com.termux/files/usr \
  -e PATH=/data/data/com.termux/files/usr/bin:/system/bin -e PROTECTORATE_HOME=.../home/state \
  test-prot-android-1 sh -c 'cd $HOME && ./protectorate node serve'      # pkg install refuses root
protectorate job test-cross --goos android --goarch arm64 ./...         # all packages pass; tmux-less and sleeve tests skip
```

Not verified on a real phone: Android 11+ restricts netlink for apps (tsnet's interface and route discovery may degrade), `/proc/stat` and `/proc/loadavg` may be unreadable (the node reports a CPU warning and drops the load average rather than failing), and battery optimisation or Doze can kill the process or freeze the network even with the wake lock. A node killed that way is shown unreachable until `node serve` runs again. Once it runs again the hub sees it fresh within a poll or two (the node pings the hub on start); no hub restart is needed. Disable battery optimisation for Termux and keep the Termux notification.

## Phone client

```
protectorate client invite --name phone      # follow the printed Tailscale app steps
protectorate client register <key>           # for the app's registration key
protectorate pair --app                      # QR for the native app; without --app, the web link
```
