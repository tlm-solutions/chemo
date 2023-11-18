# Chemo Thearpy

The Transit Live Mapping Solutions realworld cringe data processing plant.

## Purpose 

- Merge different data streams into one uniform one.
- Currently takes a R09Telegram and GPS data stream and produces a stream of waypoints.
- Chemo requires access to the Postgres database to perform the r09 reporting-point look up
- It sends out the data stream to services like [funnel](https://github.com/tlm-solutions/funnel) or [bureaucrat](https://github.com/tlm-solutions/bureaucrat)

## Service Configuration

Following Environment Variables are read by there service

- **CHEMO_HOST** Where the service accepts grpc data
- **POSTGRES_PASSWORD_PATH** where the service can read postgres credentials
- **RUST_LOG** log level
- **RUST_BACKTRACE** stack traces
- **POSTGRES_HOST** host of the postgres server
- **POSTGRES_PORT** port of the postgres server
- **POSTGRES_USER** postgres user
- **POSTGRES_DATABASE** database to use
- **GRPC_HOST_X** X can be replaced by any string where waypoints should be sent to

