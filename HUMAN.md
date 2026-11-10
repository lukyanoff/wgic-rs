# Connect RPC Rust example

A minimal `GreetService` (Connect + gRPC) server and client in Rust.
Based on https://connectrpc.com/docs/rust/getting-started/

## Requirements

- Rust 1.88+
- Node.js (for the Buf CLI): `npm install`

## Generate code

Run this after you change `greet/v1/greet.proto`:

```sh
npx buf lint
npx buf generate   # writes src/gen/buffa and src/gen/connect
```

## Run

```sh
cargo run --bin server   # terminal 1
cargo run --bin client   # terminal 2, prints "Hello, Jane!"
```

Both use `127.0.0.1:8080` by default. To use another address, set `ADDR`:

```sh
ADDR=127.0.0.1:8081 cargo run --bin server
ADDR=127.0.0.1:8081 cargo run --bin client
```

## Call it with curl

```sh
curl -H "Content-Type: application/json" -d '{"name":"Jane"}' \
  http://localhost:8080/greet.v1.GreetService/Greet
```

To call it over gRPC:

```sh
npx buf curl --schema ./greet/v1/greet.proto --protocol grpc --http2-prior-knowledge \
  --data '{"name":"Jane"}' http://localhost:8080/greet.v1.GreetService/Greet
```

## Layout

- `greet/v1/greet.proto`: the service definition
- `buf.yaml`: Buf module and lint settings
- `buf.gen.yaml`: code generation plugins
- `src/lib.rs`: pulls in the generated code as `proto::greet::v1`
- `src/bin/server.rs`, `src/bin/client.rs`: the server and the client

## Gotcha

The `connectrpc/rust` plugin needs the bare flag `file_per_package` in `buf.gen.yaml`. It fails on `file_per_package=true`.
