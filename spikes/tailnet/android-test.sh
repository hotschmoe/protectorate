#!/bin/bash
# Needs NODE_KEY; run from spikes/tailnet. Runs the android/arm64 build inside termux-docker.
docker rm -f spike-termux >/dev/null 2>&1
docker run -d --name spike-termux -v "$PWD/bin/tsnet-android-arm64:/data/local/tmp/spike:ro" \
  termux/termux-docker:aarch64 /system/bin/sh -c 'cp /data/local/tmp/spike /data/data/com.termux/files/home/spike; chmod 755 /data/data/com.termux/files/home/spike; cd /data/data/com.termux/files/home; ./spike -mode node -hostname andnode -state ./state -authkey '"$NODE_KEY"' -timeout 60s > out.log 2>&1; sleep 600' >/dev/null
sleep 20
mkdir -p .state/hub
bin/tsnet-linux-arm64 -mode call -hostname hub -state .state/hub -target http://andnode:8081/metrics -timeout 60s 2>&1 | grep -E "joined|HTTP|call:"
sleep 45
docker exec spike-termux /system/bin/sh -c 'cat /data/data/com.termux/files/home/out.log' | tail -20
docker stop spike-termux
