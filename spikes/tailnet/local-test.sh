#!/bin/bash
# Needs HUB_KEY (tag:hub) and NODE_KEY (tag:node) preauth keys; run from spikes/tailnet.
set -u
B=bin/spike-linux-arm64
S=.state
mkdir -p $S/node1 $S/hub $S/intruder
$B -mode node -hostname node1 -state $S/node1 -authkey "$NODE_KEY" -timeout 40s > $S/node1.log 2>&1 &
NP=$!
sleep 8
echo "== hub -> node1 (expect 200)"
$B -mode call -hostname hub -state $S/hub -authkey "$HUB_KEY" -target http://node1:8081/metrics -timeout 60s 2>&1 | grep -E "joined|HTTP"
echo "== intruder (tag:node) -> node1 (expect 403)"
$B -mode call -hostname intruder -state $S/intruder -authkey "$NODE_KEY" -target http://node1:8081/metrics -timeout 60s 2>&1 | grep -E "joined|HTTP"
echo "== node1 RSS while idle"
sleep 12; grep rss $S/node1.log | tail -2
wait $NP
echo "== restart node1 WITHOUT authkey (expect reuse of state)"
$B -mode node -hostname node1 -state $S/node1 -timeout 25s > $S/node1b.log 2>&1 &
NP=$!
sleep 8
$B -mode call -hostname hub -state $S/hub -target http://node1:8081/metrics -timeout 60s 2>&1 | grep -E "joined|HTTP"
wait $NP
echo "== node logs"; grep -E "joined|ACCEPT|REJECT" $S/node1.log $S/node1b.log
