# Development Documentation

This document serves as a guide to get this project working in local for development and testing.

To see what is this or see how to use this better check the main [README file](README.md).

Generally speaking this is a normal Wasm Foreign Data Wrapper. You might find it useful to check the official development guide [here](https://fdw.dev/guides/create-wasm-wrapper/).

## Project Structure

```bash
├── src/
│   ├── lib.rs              # Main FDW implementation
│   └── bindings.rs         # Generated WIT bindings
├── wit/
│   └── world.wit           # WIT world definition
├── supabase-wrappers-wit/  # Supabase WIT interface definitions
└── Cargo.toml
```

## Dependencies

You need to set up your Rust environment and install the WebAssembly Component Model:

- Install [Rust Toolchain](https://www.rust-lang.org/tools/install).
- Add `wasm32-unknown-unknown` target: `rustup target add wasm32-unknown-unknown`.
- Install the `cargo-component` model with `cargo install cargo-component --locked --version 0.21.1`. **At the time of writing this it is mandatory to use the latest version**.
- Install [docker](https://docs.docker.com/desktop/), and **make sure it's running**.
- Install [supabase cli](https://supabase.com/docs/guides/local-development/cli/getting-started). Required for local environment setup.

## Deployment

Add a version tag and push it with: `git tag -f vX.Y.Z && git push -f origin tag vX.Y.Z`.

## Spin up local environment

Start the local environment using `supabase` cli.
- `supabase init` (only needed the first time, might take a while)
- `subpabase start`
- Run `./local-dev.sh` **every time** you want to update the FDW in your container.
- Alternatively: Install `cargo-watch` with `cargo install cargo-watch` and keep `cargo watch -s ./local-dev.sh` running for auto-updating as you do your changes.

## FDW setup for local testing

You need to follow the "FDW setup" steps in [README.md](README.md) but make sure to use `'file:///gravatar_fdw.wasm'` as the `fdw_package_url` parameter.

This will make your FDW to load the WASM from within your development container.

If using `supabase` local environment you must run the queries in http://127.0.0.1:54323/project/default/sql/1

```sql
-- ... REST OF INSTRUCTIONS ON README.MD

-- MAKE SURE YOU USE 'file:///gravatar_fdw.wasm' AS `fdw_package_url`.
create server gravatar_server
  foreign data wrapper wasm_wrapper
  options (
    fdw_package_url 'file:///gravatar_fdw.wasm', -- IMPORTANT: Use this in your testing environment.
    fdw_package_name 'automattic:gravatar-fdw',
    fdw_package_version '0.2.0'
    -- ... your preferred API Key approach (see README.md)
  );

-- ... REST OF INSTRUCTIONS ON README.MD
```

## Logs

To check the logs you can tweak the log levels with:
```sql
-- Set to show INFO messages in the logs
set log_min_messages to 'info';

-- Now try your query
select * from gravatar.profiles where email = 'test@example.com';
```

And then check the logs using docker `docker logs` on the database container, or checking on the SQL Editor the Logs tab for Postgres at `http://127.0.0.1:54323/project/default/logs/postgres-logs`.

## Issues?

Please open an issue describing your problem.