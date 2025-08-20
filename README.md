# Tupac: A learning implementation of 2-Phase Commit in Rust

what it says on the box

## State of the project
We have in-memory 2-phase commit writes working. We are yet to implement READ COMMITTED, or really, any read APIs on the client nodes.

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

Then **within five seconds**, we start both clients (or more):
```
cargo run --bin client -- --sdmon-ip 127.0.0.1 --sdmon-port 3000 --port 3002
cargo run --bin client -- --sdmon-ip 127.0.0.1 --sdmon-port 3000 --port 3003
```

Now, we can use curl to write some data to the nodes. Try it out for yourself:
```
curl -XPOST -H "Content-type: application/json" -d '{"key": "hello", "value": "world"}' 'http://localhost:3001/txn_write' 
```

To read data from a node (currently, we can only read committed data):
```
curl 'http://localhost:3002/readcommitted' 
```
