#!/usr/bin/env bash
# Runs every variant and writes results.log (gitignored). Needs the docker image: docker build -t spike-iso-img:2 image
cd "$(dirname "$0")"
: > results.log
for v in ${*:-none docker bwrap bwrap-cg unshare unshare-cg}; do
  timeout 300 ./run.sh "$v" 2>&1 | grep -E '^RESULT' >> results.log
done
./fc.sh 2>&1 | grep -E '^RESULT' >> results.log
./gv.sh 2>&1 | grep -E '^RESULT' >> results.log
cat results.log
