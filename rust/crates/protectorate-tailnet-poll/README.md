# protectorate-tailnet-poll

Reconnecting, bounded Headscale map poll for a registered Rust node. Open with
the `tsnet` directory written by `protectorate-rs node join`. The poller
verifies `join.json` is a private regular file and its node and disco keys
match their durable private identities.
It uses the pinned TS2021 Noise channel for `/machine/map`, applies framed
responses to a netmap, and exposes a snapshot only while that authenticated
stream is active. Each reconnect discards the old map and requires a fresh
initial self node. A map whose self key differs from the joined key is rejected.
An optional private HTTPS CA PEM stored by `node join --ca-cert FILE` is reused
for every control connection, with normal server-name verification.

`Poller::run(&AtomicBool)` reconnects with bounded backoff. Stopping is bounded
by the channel read timeout (20 seconds by default, configurable up to 60
seconds). `Poller::poll_once` is available for integration and tests.
`subscribe()` sends the latest map through a bounded watch channel. Each new
poll increments its generation, so a packet engine can discard sessions after
a reconnect even if it missed the brief disconnected update.

This crate supplies control data only. The disco private key, packet filters,
WireGuard/DERP transport, and peer identity enforcement belong to the future
packet engine. A netmap must not by itself authorize a peer connection.

Run the disposable Headscale interoperability check with:

```sh
PROT_HEADSCALE_BINARY=/path/to/headscale-0.29.5 cargo test -p protectorate-tailnet-poll --test headscale -- --ignored
```
