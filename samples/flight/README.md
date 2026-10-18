# Flight API sample

A sample spec for a flight API. All times are in CET (UTC+01:00).

| Route | Description | Authentication |
| --- | --- | --- |
| GET `/api/flights` | Lists 10 flights with status, airline code, airport codes, times and duration. | None |
| PUT `/api/flights/{flight_code}` | Updates the flight status. | JWT bearer token |
| DELETE `/api/flights/{flight_code}` | Deletes the flight. | OAuth2 scope `privilege` |

## Run

Start Siga from the project root:

```sh
cargo run --release -- --api samples/flight/openapi.yaml
```

Send requests from a second terminal:

```sh
curl http://127.0.0.1:8080/api/flights
curl -X PUT -H 'Authorization: Bearer my-valid-token' \
  -d '{"status": "delayed"}' http://127.0.0.1:8080/api/flights/KL1234
curl -X DELETE 'http://127.0.0.1:8080/api/flights/KL1234?force-auth=privilege'
```

To get the 404 response, add `?force-error=404` to a PUT or DELETE request.
