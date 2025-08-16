# Tupac: A learning implementation of 2-Phase Commit in Rust

what it says on the box

## State of the project
As of right now, none of the APIs are built out. We only have echo/ping APIs on each node.

## Architecture
This setup comprises of three different types of services.

1. `sdmon`: Service discovery manager. Must be the first node created so we can bootstrap the cluster.
2. `coordinator`: The node in charge of performing the 2-phase commit operation. This node must be started soon after `sdmon`.
3. `client`: These are the actual database nodes. 

All nodes must register themselves with `sdmon` as soon as they start. This is to ensure that the coordinator can always get a relatively up-to-date list of all the nodes present.

## Running the project so far

Start sdmon first.
```
cargo run --bin sdmon
```
SDMon will run on `localhost:3000`.

Then we start the coordinator.
```
cargo run --bin coordinator --  --sdmon-ip 127.0.0.1 --sdmon-port 3000 --port 3001
```

Then within five seconds, we start both clients:
```
cargo run --bin client -- --sdmon-ip 127.0.0.1 --sdmon-port 3000 --port 3002
cargo run --bin client -- --sdmon-ip 127.0.0.1 --sdmon-port 3000 --port 3003
```
