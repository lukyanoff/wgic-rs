# Connect RPC Rust example

Rust Connect/gRPC servers, plus a generic web playground that can call any
method on any server.
Based on https://connectrpc.com/docs/rust/getting-started/ and
https://connectrpc.com/docs/web/getting-started/

## Requirements

- Rust 1.88+
- Node.js: run `npm install` (repo root, for the Buf CLI) and `cd web && npm install`

## Generate code

Run this after you change or add any `.proto`:

```sh
npm run gen   # buf generate (Rust) + descriptors for the playground + cargo fmt
```

## Run the server and client

```sh
cargo run --bin server   # terminal 1
cargo run --bin client   # terminal 2, prints "Hello, Jane!"
```

Both use `127.0.0.1:8080` by default. To use another address, set `ADDR`:

```sh
ADDR=127.0.0.1:8081 cargo run --bin server
ADDR=127.0.0.1:8081 cargo run --bin client
```

Servers allow CORS from `http://localhost:5173` by default. To change that, set
`CORS_ORIGINS` (comma-separated, or `*`):

```sh
CORS_ORIGINS=http://localhost:5173,https://playground.example.com cargo run --bin server
```

## Web playground

```sh
npm run gen                  # once, and after proto changes
cd web && npm run dev        # http://localhost:5173
```

- **Methods:** every service in every proto in the repo, loaded from
  `web/public/descriptors.binpb`. You don't write code per service.
- **Servers:** the defaults come from `web/public/servers.json`. You can add,
  edit and remove servers in the UI (the "Edit" button), and changes are saved
  in your browser.
  - `proxy` mode goes through the Vite dev server, so no CORS is needed. It
    only works under `npm run dev`.
  - `direct` mode calls the server straight from the browser. It needs CORS on
    the server (see `CORS_ORIGINS`).
- **Protocols:** Connect (JSON or binary) and gRPC-web. Unary and
  server-streaming methods work. Client and bidi streaming can't run in a
  browser; use `buf curl` for those.
- **Editors:** the request editor starts from a template built from the message
  fields. Press Ctrl+Enter to send. Headers go one per line as `key: value`.
- **History:** the last 20 calls. Click one to restore it.

## Add a new service

1. Add `foo/v1/foo.proto`, then run `npm run gen`.
2. Include the generated files in `src/lib.rs`, next to `greet::v1`.
3. Implement the generated `FooService` trait.
4. In a server binary:
   `serve(Router::new().add_service(Arc::new(MyFoo)), addr_from_env().parse()?).await`.
   To run more instances, start more processes with different `ADDR` values.
5. The playground lists it after a page reload.

## Call it from the command line

```sh
curl -H "Content-Type: application/json" -d '{"name":"Jane"}' \
  http://localhost:8080/greet.v1.GreetService/Greet

npx buf curl --schema . --protocol grpc --http2-prior-knowledge \
  --data '{"name":"Jane"}' http://localhost:8080/greet.v1.GreetService/Greet
```

## FYI: Buf Studio (no code)

Buf Studio is a hosted playground from Buf: https://buf.build/studio

1. Pick a schema. Either choose a module from the Buf Schema Registry, or push
   this repo's module with `npx buf push`. Studio needs the schema in the BSR.
2. Set the target URL, e.g. `http://127.0.0.1:8081`, and choose the Connect
   or gRPC-web protocol. Calls go straight from your browser, so they need the
   CORS above. Add `https://buf.build` to `CORS_ORIGINS`.
3. For plain gRPC, or to skip CORS, run a local agent:
   `npx buf beta studio-agent --port 8000`. Then set "Buf Studio Agent URL" in
   Studio to `http://localhost:8000`.

## Layout

- `greet/v1/greet.proto`: the service definition
- `buf.yaml`: Buf module and lint settings
- `buf.gen.yaml`: Rust code generation plugins
- `src/lib.rs`: generated code (`proto::...`), plus `serve()` with CORS and `addr_from_env()`
- `src/bin/server.rs`, `src/bin/client.rs`: the example server and client
- `web/`: the React playground
  - `src/schema.ts`: loads the descriptors
  - `src/call.ts`: dynamic calls
  - `src/template.ts`: request templates
  - `vite.config.ts`: the dev proxy

## Gotchas

- The `connectrpc/rust` plugin needs the bare flag `file_per_package` in
  `buf.gen.yaml`. It fails on `file_per_package=true`.
- Running `buf generate` alone leaves the generated Rust unformatted. Use
  `npm run gen`.
- On this machine, port 8080 is used by a Podman container. Use
  `ADDR=127.0.0.1:8081`.
