#!/bin/bash
# usage: hs.sh start|stop|cli args...
W="$(cd "$(dirname "$0")" && pwd)/.state/hs"
case "$1" in
start)
  docker rm spike-headscale >/dev/null 2>&1
  docker run -d --name spike-headscale --network host --user "$(id -u):$(id -g)" \
    -v $W/etc:/etc/headscale -v $W/lib:/var/lib/headscale -v $W/run:/var/run/headscale \
    headscale/headscale:v0.29.5 serve ;;
stop) docker stop spike-headscale ;;
cli) shift; docker exec spike-headscale headscale "$@" ;;
esac
