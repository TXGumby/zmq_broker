# AGENTS.md

## What this is

A ZeroMQ message broker written in Rust (`src/main.rs`) with Rust and Python publisher/subscriber clients, plus a web UI bridge. No workspace, no tests.

## Layout

- `src/main.rs` — broker (default binary, `cargo run`)
- `src/lib.rs` — shared message structs (`RegisterMessage`, `PingMessage`, `PongMessage`, `TopicListRequest`, `TopicListResponse`)
- `src/bin/publisher.rs` — Rust publisher CLI (`cargo run --bin publisher <topic>...`)
- `src/bin/subscriber.rs` — Rust subscriber CLI (`cargo run --bin subscriber [topic-filter]...`)
- `src/bin/webserver.rs` — axum HTTP+WebSocket bridge on port 3000 (`cargo run --bin webserver`)
- `static/index.html` — web UI served by webserver
- `python/pub1.py` — Python publisher; takes topics as CLI args (`python python/pub1.py news weather`)
- `python/client1.py`, `python/client_sports.py` — Python subscribers

## Build & run

```
cargo build            # debug
cargo build --release  # release
cargo run              # starts the broker
cargo run --bin publisher news weather
cargo run --bin subscriber news
cargo run --bin webserver
```

No codegen, no migrations, no pre-commit hooks, no CI config.

## ZMQ socket layout

The broker binds four TCP ports:

| Port | Socket type | Role |
|------|-------------|------|
| 5555 | XPUB | Subscribers connect SUB here |
| 5556 | XSUB | Publishers connect PUB here (publishes + register messages flow through this) |
| 5558 | ROUTER | Ping/pong channel; publishers connect DEALER here (bound by a thread spawned from `run_ping_mechanism`) |
| 5559 | REP | Topic list requests from subscribers |

The webserver binary additionally binds HTTP on port 3000.

## Message protocol

All messages are JSON. Topics are namespaced as `<publisher_uuid>:<topic>` (e.g. `abc123:news`). Topic names must not contain `:` — the subscriber filter logic in `src/bin/subscriber.rs` relies on this.

- **Register** (publisher → broker via XSUB port 5556, single frame): `{"action":"register","publisher_id":"<uuid>","topics":["t1","t2"]}`
  - The broker distinguishes register messages from normal publishes by checking whether an XSUB payload arrives as a single frame that parses as `RegisterMessage`. Regular publishes are always multi-frame (topic + data).
- **Ping** (broker → publisher via ROUTER 5558): `{"action":"ping","publisher_id":"<uuid>"}` sent every 20 seconds
- **Pong** (publisher → broker via DEALER 5558): `{"action":"pong","publisher_id":"<uuid>"}`
- **Topic list request** (subscriber → broker via REQ to REP port 5559): `{"action":"get_topics"}` (the broker does not inspect the payload and always replies with the current topic list)
- **Topic list response** (broker → subscriber): `{"action":"topic_list_response","topics":[...]}`

Publishers inactive for >60 seconds (no pong) are evicted from the registry by the main loop.

## Python clients

- `pub1.py` — publisher; takes topics as CLI args. Connects PUB to 5556, DEALER to 5558. Example: `python pub1.py news weather`
  (all paths below are relative to the `python/` directory)
- `python/client1.py` — subscriber; SUB on 5555, REQ on 5559
- `python/client_sports.py` — subscriber; filters to topics ending in `:sports`

Requires `pyzmq` (`pip install pyzmq`). No virtualenv config in the repo.

## Dependencies

- `zmq = "0.10"` — requires **libzmq** installed on the system (e.g. `libzmq3-dev` on Debian, `zeromq` via vcpkg/choco on Windows)
- `axum = "0.7"`, `tokio`, `futures-util` — used only by `src/bin/webserver.rs`
- `parking_lot`, `serde`/`serde_json`, `uuid` — pure Rust, no extra setup
- `build.rs` links `advapi32` on Windows
