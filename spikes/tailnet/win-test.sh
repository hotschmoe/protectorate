#!/bin/bash
# Needs NODE_KEY, HUB_KEY; Headscale reachable at 192.168.10.44:8080; run from spikes/tailnet.
T='C:\Users\hotschmoe\AppData\Local\Temp\protectorate-spike'
ssh officewin "\"$T\\spike.exe\" -mode node -hostname winnode -state \"$T\\state\" -authkey $NODE_KEY -timeout 70s" > .state/winnode.log 2>&1 &
WP=$!
sleep 15
mkdir -p .state/hub
bin/tsnet-linux-arm64 -mode call -hostname hub -state .state/hub -target http://winnode:8081/metrics -timeout 60s 2>&1 | grep -E "joined|HTTP|call:"
wait $WP
cat .state/winnode.log
ssh officewin 'tasklist /fi "imagename eq spike.exe" | findstr spike'
