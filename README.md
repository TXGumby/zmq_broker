# zmq_broker

A small ZeroMQ pub/sub broker written in Rust, with Rust + Python clients and a live web dashboard.

Publishers register with the broker, emit messages on namespaced topics, and respond to periodic pings. Subscribers ask the broker for the current topic list and subscribe to whatever they want. A separate web server bridges the broker to the browser over WebSocket and renders a live feed.

## Features

- XPUB/XSUB proxy for pub/sub fan-out
- Publisher registration with UUID identity
- ROUTER/DEALER ping-pong keepalive (20s interval, 60s eviction)
- REP endpoint for topic discovery
- Web dashboard (axum + WebSocket) with live feed, subscribe toggles, in-browser publishing, pause/clear, msg/s rate
- Rust and Python client implementations

## Quick start

Requires Rust (stable), Python 3.8+, and **libzmq** on the system (`zeromq` on macOS/Windows via homebrew/choco, `libzmq3-dev` on Debian/Ubuntu).

```bash
# terminal 1 — broker
cargo run

# terminal 2 — web dashboard
cargo run --bin webserver
# open http://localhost:3000

# terminal 3 — Rust publisher
cargo run --bin publisher news weather

# terminal 4 — Python publisher
python python/pub1.py sports finance

# terminal 5 — Rust subscriber (all topics)
cargo run --bin subscriber

# or filter to specific topic names
cargo run --bin subscriber sports
```

## Architecture

```
                    ┌─────────────────────────────────┐
                    │            BROKER               │
                    │                                 │
 publishers ───PUB──┤ 5556 XSUB ──> XPUB 5555 ├──SUB── subscribers
                    │                                 │
                    │ 5558 ROUTER <──DEALER──> ping  │──── publishers
                    │ 5559 REP    <──REQ──────> list │──── subscribers
                    └─────────────────────────────────┘
                                    ▲
                                    │ SUB 5555 + REQ 5559 + PUB 5556
                                    │
                              ┌─────┴──────┐
                              │ webserver  │ HTTP :3000
                              │  (axum)    │ WebSocket /ws
                              └────────────┘
```

## Ports

| Port | Socket | Purpose |
|------|--------|---------|
| 5555 | XPUB   | Subscribers connect SUB here |
| 5556 | XSUB   | Publishers connect PUB here (publishes + registration) |
| 5558 | ROUTER | Ping/pong channel; publishers connect DEALER here |
| 5559 | REP    | Topic list requests from subscribers |
| 3000 | HTTP   | Web dashboard (webserver binary only) |

## Message protocol

All messages are JSON. Topics are namespaced as `<publisher_uuid>:<topic>` (e.g. `abc12345…:news`). Topic names **must not contain `:`** — subscriber filters use it as a delimiter.

| Direction | Action | Shape |
|-----------|--------|-------|
| publisher → broker (port 5556, single frame) | `register`  | `{"action":"register","publisher_id":"<uuid>","topics":["..."]}` |
| publisher → broker (port 5556, multipart)     | publish     | frame 0: `<uuid>:<topic>`, frame 1: JSON payload |
| broker → publisher (port 5558)                | `ping`      | `{"action":"ping","publisher_id":"<uuid>"}` every 20s |
| publisher → broker (port 5558)                | `pong`      | `{"action":"pong","publisher_id":"<uuid>"}` |
| subscriber → broker (port 5559)               | `get_topics`| `{"action":"get_topics"}` |
| broker → subscriber (port 5559)               | response    | `{"action":"topic_list_response","topics":["..."]}` |

Publishers that miss pongs for >60s are evicted from the registry.

The broker distinguishes a `register` message from a regular publish by frame count: single-frame XSUB payloads that parse as a `RegisterMessage` are treated as registration, everything else is forwarded to subscribers as-is.

## Layout

```
src/
  main.rs            broker (default binary: `cargo run`)
  lib.rs             shared message structs
  bin/
    publisher.rs     Rust publisher CLI
    subscriber.rs    Rust subscriber CLI
    webserver.rs     axum HTTP + WebSocket bridge
static/
  index.html         web dashboard (glassmorphism UI)
python/
  pub1.py            Python publisher (topics as CLI args)
  client1.py         Python subscriber (all topics)
  client_sports.py   Python subscriber (:sports filter)
```

## Dependencies

Rust crates (see `Cargo.toml`):
- `zmq 0.10` — requires libzmq at link time
- `axum 0.7` + `tokio` + `futures-util` — webserver only
- `serde`, `serde_json`, `parking_lot`, `uuid`

Python: `pyzmq` (`pip install pyzmq`).

## Development notes

- No test suite yet
- Startup errors panic by design; runtime loops log and continue
- `AGENTS.md` contains agent-oriented build/run notes and is kept in sync with this README
