//! Starts the service. Configuration comes from the environment, see `docs/specs/cloud.md`.
#![allow(clippy::print_stderr)]

use tronic_cloud::{connect, router, Config, Store};

fn required(name: &str) -> Result<String, String> {
    std::env::var(name).map_err(|_| format!("{name} is not set"))
}

/// `tronic-cloud create-account <email>` makes the first account of a server with
/// closed registration. The password comes from `TRONIC_NEW_PASSWORD`, so that it
/// does not end up in the shell history.
async fn create_account(database: &str, email: &str) -> Result<(), String> {
    let password = required("TRONIC_NEW_PASSWORD")?;
    // The store is not touched by this.
    let state = connect(
        database,
        Store::folder(std::env::temp_dir()),
        Config {
            default_quota: quota(),
            allow_registration: true,
        },
    )
    .await?;
    tronic_cloud::add_account(&state, email, &password)
        .await
        .map_err(|error| format!("{error:?}"))?;
    eprintln!("account created: {email}");
    Ok(())
}

fn quota() -> i64 {
    std::env::var("TRONIC_QUOTA_BYTES")
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(5 * 1024 * 1024 * 1024)
}

async fn run() -> Result<(), String> {
    let database = required("DATABASE_URL")?;
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if let [command, email] = &arguments[..] {
        if command == "create-account" {
            return create_account(&database, email).await;
        }
    }
    if !arguments.is_empty() {
        return Err("usage: tronic-cloud [create-account <email>]".to_owned());
    }
    let bind = std::env::var("TRONIC_CLOUD_BIND").unwrap_or_else(|_| "127.0.0.1:8787".to_owned());
    let store = match std::env::var("TRONIC_S3_ENDPOINT") {
        Ok(endpoint) => Store::s3(
            &endpoint,
            &required("TRONIC_S3_BUCKET")?,
            &std::env::var("TRONIC_S3_REGION").unwrap_or_default(),
            &required("TRONIC_S3_ACCESS_KEY")?,
            &required("TRONIC_S3_SECRET_KEY")?,
        )
        .map_err(|error| error.to_string())?,
        Err(_) => Store::folder(required("TRONIC_STORE_DIR")?),
    };
    let config = Config {
        default_quota: quota(),
        allow_registration: std::env::var("TRONIC_ALLOW_REGISTRATION").as_deref() != Ok("false"),
    };
    let state = connect(&database, store, config).await?;
    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .map_err(|error| error.to_string())?;
    eprintln!("listening on {bind}. Run this behind a proxy that terminates TLS.");
    axum::serve(listener, router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .map_err(|error| error.to_string())
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
