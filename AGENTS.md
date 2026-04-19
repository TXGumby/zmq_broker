# AGENTS.md

## What this is

A ZeroMQ message broker written in Rust (`src/main.rs`) with Python publisher/subscriber clients. Single binary, no workspace, no tests.

## Build & run

```
cargo build            # debug
cargo build --release  # release
cargo run              # starts the broker
```

No codegen, no migrations, no pre-commit hooks, no CI config.

## ZMQ socket layout

The broker binds four ports. All clients must target these exact ports:

| Port | Socket type | Role |
|------|-------------|------|
| 5555 | XPUB | Subscribers connect here (also used by `client1.py` REQ for topic list) |
| 5556 | XSUB | Publishers connect here to publish / register |
| 5557 | ROUTER | Unused in current ping flow (bound but ping thread uses 5558) |
| 5558 | ROUTER | Ping thread binds here; publishers connect DEALER here for pong |

**Note:** There is a latent bug — `Broker::new` binds a ROUTER on 5557, but `run_ping_mechanism` spawns a thread that binds a second ROUTER on 5558. If you touch the ping/pong flow, keep both sockets in mind.

## Message protocol

All messages are JSON. Topics are namespaced as `<publisher_uuid>:<topic>` (e.g. `abc123:news`).

- **Register** (publisher → broker via XSUB port 5556): `{"action":"register","publisher_id":"<uuid>","topics":["t1","t2"]}`
- **Ping** (broker → publisher via ROUTER 5558): `{"action":"ping","publisher_id":"<uuid>"}`
- **Pong** (publisher → broker via DEALER 5558): `{"action":"pong","publisher_id":"<uuid>"}`
- **Topic list request** (subscriber → broker via XPUB 5555): `{"action":"get_topics"}`
- **Topic list response** (broker → subscriber): `{"action":"topic_list_response","topics":[...]}`

Publishers inactive for >60 seconds (no pong) are evicted from the registry.

## Python clients

- `pub1.py` — publisher; connects PUB to 5556, DEALER to 5558; publishes `news` and `weather` topics
- `python/client1.py` — subscriber; connects SUB and REQ both to 5555

Requires `pyzmq` (`pip install pyzmq`). No virtualenv config is present in the repo.

## Dependencies

- `zmq = "0.10"` — requires **libzmq** installed on the system (e.g. `libzmq3-dev` on Debian, `zeromq` via vcpkg/choco on Windows)
- `parking_lot`, `serde`/`serde_json`, `uuid` — pure Rust, no extra setup
