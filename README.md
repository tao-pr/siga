# Siga

Siga is a mock server for REST APIs. You give it an OpenAPI 3 spec file in YAML or JSON. Siga reads the file and starts an HTTP server that answers every route in the spec. Each response contains random data that follows the response schema in the spec.

Use Siga to build or test an API client before the real API exists.

## Features

- Siga serves all OpenAPI methods: GET, PUT, POST, DELETE, OPTIONS, HEAD, PATCH and TRACE.
- Siga validates JSON request bodies against the spec. It returns 400 if a body does not match.
- Siga generates random JSON response bodies from the response schemas.
- Siga emulates API key, bearer, JWT and OAuth2 authentication.
- Siga returns any status code that you ask for with `?force-error`.

## Requirements

You need Rust 1.88 or later, with Cargo.

## Build

Build the release binary:

```sh
cargo build --release
```

Cargo writes the binary to `target/release/siga`.

## Run

1. Write an OpenAPI 3 spec file, or use the sample spec `tests/data/petstore.yaml`.
2. Start Siga with the `--api` flag and the path to the spec file:

   ```sh
   ./target/release/siga --api tests/data/petstore.yaml
   ```

   You can also build and start Siga in one step:

   ```sh
   cargo run --release -- --api tests/data/petstore.yaml
   ```

3. Read the address and the routes that Siga prints:

   ```text
   Siga emulates "Petstore" on http://127.0.0.1:8080/v1
     GET     /v1/pets
     POST    /v1/pets
     GET     /v1/pets/count
     GET     /v1/pets/{petId}
     PUT     /v1/pets/{petId}
     DELETE  /v1/pets/{petId}
     GET     /v1/health
   ```

4. Send a request from a second terminal:

   ```sh
   curl -H 'X-API-Key: my-valid-key' http://127.0.0.1:8080/v1/pets/count
   ```

   Siga answers with random data, for example `{"count":7}`.

5. To stop the server, press Ctrl+C.

If Siga cannot use the spec, it prints a list of the problems and stops. Correct each problem, then start Siga again.

Siga listens on 127.0.0.1 only. Other computers and Docker containers cannot connect to it.

## Spec rules

Siga reads OpenAPI 3.0 specs from `.yaml`, `.yml` or `.json` files. The spec must obey these rules:

- The first `servers` URL must include a port, for example `http://localhost:8080/v1`. Siga listens on that port and serves the routes under the URL path, here `/v1`.
- Each path must have at least one operation.
- Each operation must define a `"200"` response.
- Each GET, POST, PUT and PATCH operation must define a JSON schema for its `"200"` response.
- Each POST, PUT and PATCH operation must define a JSON request body schema.
- Each `$ref` must point to an item in `components`.
- Each schema must declare a `type`, or use `oneOf`, `anyOf` or `allOf`.

Siga rejects a spec that uses one of these features:

- free-form objects (objects with no defined properties)
- `not` schemas
- callbacks
- `$ref` on a path item
- HTTP authentication schemes other than bearer
- OpenID Connect

## Generated data

Siga fills every field of a response schema, including fields that are not in `required`. It generates each value from the schema type:

- Integer: a random integer from `minimum` to `maximum`. Without limits, the range is 0 to 1000.
- Number: a random number with two decimal places, with the same range rules.
- String: one to three random English words, cut to `maxLength`.
- Boolean: `true` or `false`.
- Enum: a random value from the enum.
- Array: 3 items, or `minItems` items if the schema sets it. If `uniqueItems` is true, each item is different.
- Object: every property in the schema.
- `oneOf` and `anyOf`: one schema from the list, picked at random.
- `allOf`: the fields of all the schemas in the list, merged into one object.

A schema can refer to itself, for example a tree node with child nodes. Siga stops at a nesting depth of 10 and writes `null` at that depth.

## Authentication

Siga reads the `security` list of the operation. If the operation has no `security` list, Siga uses the list at the root of the spec. If neither list exists, or the list is empty, every request passes. If authentication fails, Siga returns 401.

- API key: send `my-valid-key` in the header, query parameter or cookie that the scheme names. Any other value fails, and a missing key fails.
- Bearer and JWT: send `Authorization: Bearer my-valid-token`. Any other token fails, and a missing token fails. This rule applies with or without `bearerFormat: JWT`.
- OAuth2: add `?force-auth=` with a comma-separated list of granted scopes, for example `?force-auth=read,write`. The request passes if the list contains every scope that the route requires. `?force-auth=no` fails, and a missing `force-auth` fails.

Example of an OAuth2 request:

```sh
curl -X POST 'http://127.0.0.1:8080/v1/pets?force-auth=write' \
  -d '{"name": "rex", "status": "sold"}'
```

## Forced status codes

Add `?force-error={code}` to a request to get that status code:

```sh
curl -i -H 'X-API-Key: my-valid-key' 'http://127.0.0.1:8080/v1/pets?force-error=503'
```

Siga looks for a response for that code in the spec. It tries the exact code first, then the range (for example `5XX`), then `default`. If it finds a response with a JSON schema, the body follows that schema. If not, the body is empty.

Siga checks authentication before it applies `force-error`. A request that fails authentication gets 401, whatever code it asks for.

## Request validation

Siga checks the JSON request body against the request body schema. It checks types, required fields, enums, number limits, string length, array length and `uniqueItems`. If the body does not match, Siga returns 400 with the first problem it finds:

```json
{"error": "body.status: required field is missing"}
```

If the request body is optional and the request has no body, Siga does not check it.

Siga also returns these errors:

- 404 if the path is not in the spec.
- 405 if the spec does not define the method for the path.
- 400 if the `force-error` value is not a status code.

## Test

Run all the tests:

```sh
cargo test
```

## Project layout

- `src/main.rs`: the command line, with the `--api` flag.
- `src/spec.rs`: loads the spec file and checks that Siga can use it.
- `src/schema.rs`: generates random data and validates request bodies.
- `src/auth.rs`: emulates the authentication schemes.
- `src/server.rs`: matches routes and answers requests.
- `tests/`: integration tests and sample specs in `tests/data/`.
