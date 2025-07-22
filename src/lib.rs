// Copyright 2025 Automattic
//
// This file is part of Gravatar Wasm Foreign Data Wrapper which is licensed under
// the GNU General Public License v3.0.
#[allow(warnings)]
mod bindings;
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};

use bindings::{
    exports::supabase::wrappers::routines::Guest,
    supabase::wrappers::{
        http,
        time,
        types::{Cell, Context, FdwError, FdwResult, ImportForeignSchemaStmt, OptionsType, Row, TypeOid, Value},
        utils,
    },
};

#[derive(Debug, Default)]
struct GravatarFdw {
    base_url: String,
    headers: Vec<(String, String)>,
    scanned_profiles: Vec<JsonValue>,
    scan_index: usize,
}

// pointer for the static FDW instance
static mut INSTANCE: *mut GravatarFdw = std::ptr::null_mut::<GravatarFdw>();

impl GravatarFdw {
    const PROFILES_OBJECT: &'static str = "profiles";

    // initialise FDW instance
    fn init_instance() {
        let instance = Self::default();
        unsafe {
            INSTANCE = Box::leak(Box::new(instance));
        }
    }

    fn this_mut() -> &'static mut Self {
        unsafe { &mut (*INSTANCE) }
    }

    // Hash email using SHA-256
    fn hash_email(email: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(email.trim().to_lowercase().as_bytes());
        format!("{:x}", hasher.finalize())
    }

    // Build URL for gravatar profile
    fn build_url(&self, email: &str) -> String {
        let hash = Self::hash_email(email);
        format!("{}/{}", self.base_url, hash)
    }
}

impl Guest for GravatarFdw {
    fn host_version_requirement() -> String {
        // semver expression for Wasm FDW host version requirement
        // ref: https://docs.rs/semver/latest/semver/enum.Op.html
        "^0.1.0".to_string()
    }

    fn init(ctx: &Context) -> FdwResult {
        Self::init_instance();
        let this = Self::this_mut();

        let opts = ctx.get_options(&OptionsType::Server);
        this.base_url = opts.require_or("api_url", "https://api.gravatar.com/v3/profiles");

        // Initialize basic headers
        let user_agent = format!("{}/{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        this.headers.push(("user-agent".to_owned(), user_agent));
        this.headers.push(("accept".to_owned(), "application/json".to_owned()));

        // Handle API key authentication
        // Support two options: direct api_key or api_key_id (vault UUID)
        if let Some(api_key) = opts.get("api_key") {
            // Direct API key provided (not recommended for production)
            this.headers.push(("authorization".to_owned(), format!("Bearer {}", api_key)));
            utils::report_info("Gravatar FDW initialized with direct API key");
        } else if let Some(api_key_id) = opts.get("api_key_id") {
            // Get API key from Vault using UUID
            let vault_api_key = utils::get_vault_secret(&api_key_id).unwrap_or_default();
            if !vault_api_key.is_empty() {
                this.headers.push(("authorization".to_owned(), format!("Bearer {}", vault_api_key)));
                utils::report_info("Gravatar FDW initialized with API key from Vault");
            } else {
                return Err(format!("Failed to retrieve API key from Vault using ID: {}", api_key_id));
            }
        } else {
            // No API key provided - will use public API endpoints only
            utils::report_info("Gravatar FDW initialized without API key (public access only)");
        }

        utils::report_info(&format!("Gravatar FDW initialized with base URL: {}", this.base_url));

        Ok(())
    }

    fn begin_scan(ctx: &Context) -> FdwResult {
        let this = Self::this_mut();

        // Clear previous results
        this.scanned_profiles.clear();
        this.scan_index = 0;

        let opts = ctx.get_options(&OptionsType::Table);
        let table = opts.require_or("table", Self::PROFILES_OBJECT);

        if table != Self::PROFILES_OBJECT {
            return Err(format!("Unsupported table '{}'. Only 'profiles' is supported.", table));
        }

        // Look for email filters in quals
        let mut emails_to_fetch = Vec::new();
        let quals = ctx.get_quals();
        
        for qual in quals {
            if qual.field() == "email" {
                if qual.operator() == "=" {
                    if let Value::Cell(Cell::String(email)) = qual.value() {
                        emails_to_fetch.push(email);
                    }
                } else {
                    // Handle unsupported operators like IN, LIKE, etc.
                    return Err(format!("Unsupported operator '{}' for email field. Only '=' (equality) is supported.", qual.operator()));
                }
            }
        }

        // If no email filter provided, we can't fetch profiles
        if emails_to_fetch.is_empty() {
            utils::report_info("No email filters provided. Gravatar FDW requires email = 'email@example.com' in WHERE clause");
            return Ok(());
        }

        // Only allow one email at a time
        if emails_to_fetch.len() > 1 {
            return Err(format!("Multiple email filters are not supported. Found {} email conditions. Use separate queries for each email.", emails_to_fetch.len()));
        }

        // Fetch profiles for each email
        for email in emails_to_fetch {
            let url = this.build_url(&email);

            let req = http::Request {
                method: http::Method::Get,
                url,
                headers: this.headers.clone(),
                body: String::default(),
            };

            let resp = http::get(&req)?;

            // Handle 429 rate limiting
            if resp.status_code == 429 {
                // Check if we're using an API key
                let using_api_key = this.headers.iter().any(|(key, _)| key.to_lowercase() == "authorization");

                // Build error message based on X-RateLimit-Reset header and API key usage
                let mut error_msg = "Rate limit exceeded (429).".to_string();

                if let Some(reset_header) = resp.headers.iter().find(|h| h.0.to_lowercase() == "x-ratelimit-reset") {
                    if let Ok(reset_timestamp) = reset_header.1.parse::<u64>() {
                        let current_time = time::epoch_secs() as u64;
                        let wait_seconds = if reset_timestamp > current_time {
                            reset_timestamp - current_time
                        } else {
                            0
                        };
                        error_msg.push_str(&format!(" Wait {} seconds for reset.", wait_seconds));
                    }
                }

                if using_api_key {
                    error_msg.push_str(" Please contact Gravatar to increase your usage limit.");
                } else {
                    error_msg.push_str(" Consider getting an API key at https://gravatar.com/developers/applications for higher rate limits.");
                }

                return Err(error_msg);
            }

            if resp.status_code == 200 {
                // Parse successful response
                let mut profile: JsonValue = serde_json::from_str(&resp.body)
                    .map_err(|e| format!("Failed to parse JSON response: {}", e))?;

                // Add email to the response since API doesn't return it
                if let JsonValue::Object(ref mut map) = profile {
                    map.insert("email".to_string(), JsonValue::String(email.clone()));
                }

                this.scanned_profiles.push(profile);
            } else {
                // Handle 404 (expected for private or non-existing profiles) and generic API errors
                // by skipping this email - no row will be returned for failed lookups
                if resp.status_code == 404 {
                    utils::report_info(&format!("Profile not found for email: {}", email));
                } else {
                    utils::report_info(&format!("HTTP error {} for email {}: {}", resp.status_code, email, resp.body));
                }
            }
        }

        utils::report_info(&format!("Found {} profiles", this.scanned_profiles.len()));

        Ok(())
    }

    fn iter_scan(ctx: &Context, row: &Row) -> Result<Option<u32>, FdwError> {
        let this = Self::this_mut();

        if this.scan_index >= this.scanned_profiles.len() {
            return Ok(None);
        }

        let profile = &this.scanned_profiles[this.scan_index];

        for tgt_col in ctx.get_columns() {
            let tgt_col_name = tgt_col.name();
            let cell = match tgt_col_name.as_str() {
                "hash" => profile.get("hash").and_then(|v| v.as_str()).map(|s| Cell::String(s.to_string())),
                "email" => profile.get("email").and_then(|v| v.as_str()).map(|s| Cell::String(s.to_string())),
                "display_name" => profile.get("display_name").and_then(|v| v.as_str()).map(|s| Cell::String(s.to_string())),
                "profile_url" => profile.get("profile_url").and_then(|v| v.as_str()).map(|s| Cell::String(s.to_string())),
                "avatar_url" => profile.get("avatar_url").and_then(|v| v.as_str()).map(|s| Cell::String(s.to_string())),
                "avatar_alt_text" => profile.get("avatar_alt_text").and_then(|v| v.as_str()).map(|s| Cell::String(s.to_string())),
                "location" => profile.get("location").and_then(|v| v.as_str()).map(|s| Cell::String(s.to_string())),
                "description" => profile.get("description").and_then(|v| v.as_str()).map(|s| Cell::String(s.to_string())),
                "job_title" => profile.get("job_title").and_then(|v| v.as_str()).map(|s| Cell::String(s.to_string())),
                "company" => profile.get("company").and_then(|v| v.as_str()).map(|s| Cell::String(s.to_string())),
                "verified_accounts" => profile.get("verified_accounts").map(|v| Cell::Json(v.to_string())),
                "pronunciation" => profile.get("pronunciation").and_then(|v| v.as_str()).map(|s| Cell::String(s.to_string())),
                "pronouns" => profile.get("pronouns").and_then(|v| v.as_str()).map(|s| Cell::String(s.to_string())),
                "timezone" => profile.get("timezone").and_then(|v| v.as_str()).map(|s| Cell::String(s.to_string())),
                "languages" => profile.get("languages").map(|v| Cell::Json(v.to_string())),
                "first_name" => profile.get("first_name").and_then(|v| v.as_str()).map(|s| Cell::String(s.to_string())),
                "last_name" => profile.get("last_name").and_then(|v| v.as_str()).map(|s| Cell::String(s.to_string())),
                "is_organization" => profile.get("is_organization").and_then(|v| v.as_bool()).map(|s| Cell::Bool(s)),
                "links" => profile.get("links").map(|v| Cell::Json(v.to_string())),
                "interests" => profile.get("interests").map(|v| Cell::Json(v.to_string())),
                "payments" => profile.get("payments").map(|v| Cell::Json(v.to_string())),
                "contact_info" => profile.get("contact_info").map(|v| Cell::Json(v.to_string())),
                "number_verified_accounts" => profile.get("number_verified_accounts").and_then(|v| v.as_i64()).map(|s| Cell::I64(s)),
                "last_profile_edit" => profile.get("last_profile_edit").and_then(|v| v.as_str()).map(|s| Cell::String(s.to_string())), // Postgres is converting string to timestamp.
                "registration_date" => profile.get("registration_date").and_then(|v| v.as_str()).map(|s| Cell::String(s.to_string())), // Postgres is converting string to timestmap.
                "json" => Some(Cell::Json(profile.to_string())),
                _ => {
                    // For unknown columns, try to get the value directly
                    match tgt_col.type_oid() {
                        TypeOid::Bool => profile.get(&tgt_col_name).and_then(|v| v.as_bool()).map(Cell::Bool),
                        TypeOid::String => profile.get(&tgt_col_name).and_then(|v| v.as_str()).map(|s| Cell::String(s.to_string())),
                        TypeOid::I32 => profile.get(&tgt_col_name).and_then(|v| v.as_i64()).map(|i| Cell::I32(i as i32)),
                        TypeOid::I64 => profile.get(&tgt_col_name).and_then(|v| v.as_i64()).map(Cell::I64),
                        TypeOid::Json => profile.get(&tgt_col_name).map(|v| Cell::Json(v.to_string())),
                        _ => None,
                    }
                }
            };

            row.push(cell.as_ref());
        }

        this.scan_index += 1;

        Ok(Some(0))
    }

    fn re_scan(_ctx: &Context) -> FdwResult {
        let this = Self::this_mut();
        this.scan_index = 0;
        Ok(())
    }

    fn end_scan(_ctx: &Context) -> FdwResult {
        let this = Self::this_mut();
        this.scanned_profiles.clear();
        this.scan_index = 0;
        Ok(())
    }

    fn begin_modify(_ctx: &Context) -> FdwResult {
        Err("modify on foreign table is not supported".to_owned())
    }

    fn insert(_ctx: &Context, _row: &Row) -> FdwResult {
        Ok(())
    }

    fn update(_ctx: &Context, _rowid: Cell, _row: &Row) -> FdwResult {
        Ok(())
    }

    fn delete(_ctx: &Context, _rowid: Cell) -> FdwResult {
        Ok(())
    }

    fn end_modify(_ctx: &Context) -> FdwResult {
        Ok(())
    }

    fn import_foreign_schema(
        _ctx: &Context,
        stmt: ImportForeignSchemaStmt,
    ) -> Result<Vec<String>, FdwError> {
        let ret = vec![format!(
            r#"create foreign table if not exists profiles (
                    hash text,
                    email text,
                    display_name text,
                    profile_url text,
                    avatar_url text,
                    avatar_alt_text text,
                    location text,
                    description text,
                    job_title text,
                    company text,
                    verified_accounts jsonb,
                    pronunciation text,
                    pronouns text,
                    timezone text,
                    languages jsonb,
                    first_name text,
                    last_name text,
                    is_organization boolean,
                    links jsonb,
                    interests jsonb,
                    payments jsonb,
                    contact_info jsonb,
                    number_verified_accounts bigint,
                    last_profile_edit timestamp,
                    registration_date timestamp,
                    json jsonb
                )
                server {} options (
                    table 'profiles'
                )"#,
            stmt.server_name,
        )];
        Ok(ret)
    }
}

bindings::export!(GravatarFdw with_types_in bindings);
