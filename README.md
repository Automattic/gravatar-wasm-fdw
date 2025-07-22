# Gravatar WASM Foreign Data Wrapper

A PostgreSQL Foreign Data Wrapper (FDW) for accessing Gravatar profile data, implemented as a WebAssembly component
using the [Supabase Wrappers framework](https://github.com/supabase/wrappers).

## Development

Looking for development documentation? Check [DEVELOPMENT.md](DEVELOPMENT.md).

## Features

- Query Gravatar profiles by email address
- Returns profile information including display name, avatar URL, location, etc.
- Email hashing using SHA-256 (as required by Gravatar API)
- Support for API key authentication via Supabase Vault extension
- Automatic fallback to public API when no API key is provided

## FDW Setup

To use this FDW you need to enable the wrappers extension, create the `gravatar_server` and create the schema.

All of the above can be done by following the following SQL snippet.

> [!IMPORTANT]  
> To get the most up-to-date values for `fdw_package_url`, `fdw_package_version` and `fdw_package_checksum`
> check [the README.txt file from the latest release](https://github.com/Automattic/gravatar-wasm-fdw/releases/latest/download/README.txt).

```sql
-- Install Wrappers extension

create
extension if not exists wrappers with schema extensions;

create
foreign data wrapper wasm_wrapper
  handler wasm_fdw_handler
  validator wasm_fdw_validator;

-- Create gravatar_server

-- Recommended: Add Gravatar FDW server (with API key from Vault)
select vault.create_secret('your-gravatar-api-key-value-goes-here', 'gravatar-api-key');
create server gravatar_server
  foreign data wrapper wasm_wrapper
  options (
    fdw_package_url 'https://github.com/Automattic/gravatar-wasm-fdw/releases/download/v0.2.0/gravatar_fdw.wasm',
    fdw_package_name 'automattic:gravatar-fdw',
    fdw_package_version '0.2.0',
    fdw_package_checksum '5273ae07e66bc2f1bb5a23d7b9e0342463971691e587bbd6f9466814a8bac11c',
    api_key_id 'your-vault-secret-uuid-here'
  );

-- Alternative 1: Direct API key (not recommended for production)
-- create server gravatar_server
--   foreign data wrapper wasm_wrapper
--   options (
--     fdw_package_url 'https://github.com/Automattic/gravatar-wasm-fdw/releases/download/v0.2.0/gravatar_fdw.wasm',
--     fdw_package_name 'automattic:gravatar-fdw',
--     fdw_package_version '0.2.0',
--     fdw_package_checksum '5273ae07e66bc2f1bb5a23d7b9e0342463971691e587bbd6f9466814a8bac11c',
--     api_key 'your-direct-api-key-here'
--   );

-- Alternative 2: No API key (only for development, rate limited)
-- create server gravatar_server
--   foreign data wrapper wasm_wrapper
--   options (
--     fdw_package_url 'https://github.com/Automattic/gravatar-wasm-fdw/releases/download/v0.2.0/gravatar_fdw.wasm',
--     fdw_package_name 'automattic:gravatar-fdw',
--     fdw_package_version '0.2.0',
--     fdw_package_checksum '5273ae07e66bc2f1bb5a23d7b9e0342463971691e587bbd6f9466814a8bac11c'
--   );

-- Create required schemas

-- Optional: Delete existing schema
-- drop schema gravatar cascade;

-- Create schema and tables
create schema if not exists gravatar;

-- Create tables by importing foreign schemas
import foreign schema gravatar
  from server gravatar_server
  into gravatar;

```

## Usage

The FDW requires an email filter in your queries. You cannot scan all profiles without specifying an email:

### Query Single Profile

```sql
SELECT *
FROM gravatar.profiles
WHERE email = 'user@example.com';
```

## Column Descriptions

| Column                     | Type      | Description                                       |
|----------------------------|-----------|---------------------------------------------------|
| `hash`                     | text      | SHA-256 hash of the email (used by Gravatar API)  |
| `email`                    | text      | Email address (added by FDW, not returned by API) |
| `display_name`             | text      | Display name                                      |
| `profile_url`              | text      | URL to the Gravatar profile page                  |
| `avatar_url`               | text      | URL to the avatar image                           |
| `avatar_alt_text`          | text      | Alt text for the avatar image                     |
| `location`                 | text      | Location                                          |
| `description`              | text      | Profile description/bio                           |
| `job_title`                | text      | Job title                                         |
| `company`                  | text      | Company                                           |
| `verified_accounts`        | jsonb     | Verified social media accounts                    |
| `pronunciation`            | text      | Pronunciation guide for the user's name           |
| `pronouns`                 | text      | User's pronouns                                   |
| `timezone`                 | text      | User's timezone                                   |
| `languages`                | jsonb     | Languages spoken by the user                      |
| `first_name`               | text      | First name                                        |
| `last_name`                | text      | Last name                                         |
| `is_organization`          | bool      | Whether this is an organization profile           |
| `links`                    | jsonb     | Social media and website links                    |
| `interests`                | jsonb     | User's interests and hobbies                      |
| `payments`                 | jsonb     | Payment methods and donation links                |
| `contact_info`             | jsonb     | Contact information                               |
| `number_verified_accounts` | int       | Number of verified social media accounts          |
| `last_profile_edit`        | timestamp | Date and time of last profile edit                |
| `registration_date`        | timestamp | Account registration date                         |
| `json`                     | jsonb     | Complete profile data as returned by API          |

## Error Handling

- **Profile not found (404)**: Returns no rows (expected for private or non-existing profiles)
- **API errors**: Returns no rows, logs error details
- **No email filter**: Returns empty result set with informational message
- **Rate Limit**: Returns error with details on the time to wait and how to get higher rate limits

## Limitations

- Requires email filters in WHERE clause (cannot scan without email)
- Only supports retrieving a single email per query
  - Use separate queries for each email address
  - Multiple email conditions will return an error (when detected – see below)
  - Using `OR` like `email = 'a@example.com' OR email = 'b@example.com'` is not supported and _most likely_ will return zero results. This is a limitation on Wrappers library in which our FDW implementation does not receive any WHERE clauses.
- Read-only (no INSERT/UPDATE/DELETE operations)
- Any request failure implies three retries with exponential backoff.
    - This is Wrapper's default behaviour and can't be disabled.
    - Specially annoying for Rate Limit errors (HTTP 429).

## License

This project is licensed under the GNU General Public License v3.0 - see the [LICENSE](LICENSE) file for details.

This project is based on the [postgres-wasm-fdw](https://github.com/supabase-community/postgres-wasm-fdw) template,
which is licensed under Apache 2.0. The original template code remains under Apache 2.0 license - see
the [LICENSE-APACHE](LICENSE-APACHE) file for details.

The combined work is distributed under GPL v3.0, which is compatible with Apache 2.0.
