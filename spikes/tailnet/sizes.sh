#!/bin/bash
# Build size matrix. Run from spikes/tailnet.
export PATH="$HOME/.local/go/bin:$PATH" CGO_ENABLED=0
mkdir -p bin
for t in linux/arm64 linux/amd64 windows/amd64 windows/arm64 android/arm64; do
  os=${t%/*}; arch=${t#*/}; ext=; [ $os = windows ] && ext=.exe
  for p in baseline:. wg:./wg tsnet:.; do
    name=${p%%:*}; dir=${p#*:}
    [ $name = baseline ] && dir=./baseline
    out=bin/$name-$os-$arch$ext
    if GOOS=$os GOARCH=$arch go build -trimpath -ldflags "-s -w" -o $out $dir 2>bin/err.txt; then
      echo "$name $t $(stat -c %s $out)"
    else
      echo "$name $t FAIL $(head -c 200 bin/err.txt)"
    fi
  done
done
