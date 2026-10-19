#!/bin/bash
# Local wireguard-go netstack hub/spoke test. Run from spikes/tailnet after building bin/wg-test.
B=bin/wg-test
H=$($B -mode genkey); S=$($B -mode genkey)
hp=$(echo "$H" | sed -n 's/private=//p'); hP=$(echo "$H" | sed -n 's/public=//p')
sp=$(echo "$S" | sed -n 's/private=//p'); sP=$(echo "$S" | sed -n 's/public=//p')
$B -mode spoke -ip 10.99.0.2 -priv "$sp" -peer-pub "$hP" -peer-ip 10.99.0.1 -endpoint 127.0.0.1:51820 -timeout 20s > .state/wg-spoke.log 2>&1 &
SP=$!
sleep 1
$B -mode hub -ip 10.99.0.1 -priv "$hp" -peer-pub "$sP" -peer-ip 10.99.0.2 -listen 51820 -target http://10.99.0.2:8081/metrics
grep VmRSS /proc/$SP/status
wait $SP
cat .state/wg-spoke.log
