//! Telegram authentication: config storage, session management, interactive login.
//!
//! Original from telegram-mcp (MIT, (c) 2026 septagram).

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use grammers_client::{Client, SignInError};
use grammers_mtsender::SenderPool;
use grammers_session::storages::SqliteSession;
use serde::{Deserialize, Serialize};

/// Where all telegram-mcp data lives.
fn data_dir() -> Result<PathBuf> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .context("Neither USERPROFILE nor HOME is set")?;
    let dir = PathBuf::from(home).join(".telegram-mcp");
    std::fs::create_dir_all(&dir).with_context(|| format!("Failed to create {}", dir.display()))?;
    Ok(dir)
}

fn session_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("session.db"))
}

fn config_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("config.json"))
}

#[derive(Serialize, Deserialize)]
struct Config {
    api_id: i32,
    api_hash: String,
}

fn store_config(api_id: i32, api_hash: &str) -> Result<()> {
    let config = Config {
        api_id,
        api_hash: api_hash.to_string(),
    };
    let json = serde_json::to_string_pretty(&config)?;
    std::fs::write(config_path()?, json).context("Failed to write config")?;
    Ok(())
}

fn load_config() -> Result<Config> {
    let path = config_path()?;
    let json = std::fs::read_to_string(&path).with_context(|| {
        format!(
            "No config at {} — run `telegram-mcp --auth` first",
            path.display()
        )
    })?;
    serde_json::from_str(&json).context("Failed to parse config")
}

/// Connect to Telegram using a saved session.
pub async fn connect() -> Result<(Client, tokio::task::JoinHandle<()>)> {
    let config = load_config()?;
    let path = session_path()?;

    if !path.exists() {
        bail!(
            "No session file at {} — run `telegram-mcp --auth` first",
            path.display()
        );
    }

    let path_str = path
        .to_str()
        .context("Session path contains invalid UTF-8")?;
    let session = Arc::new(SqliteSession::open(path_str).await?);

    let SenderPool {
        runner,
        updates: _updates,
        handle,
    } = SenderPool::new(Arc::clone(&session), config.api_id);

    let client = Client::new(handle);
    let pool_task = tokio::spawn(async move { runner.run().await });

    if !client.is_authorized().await? {
        bail!("Session exists but is not authorized — run `telegram-mcp --auth` again");
    }

    Ok((client, pool_task))
}

/// Interactive authentication flow.
pub async fn interactive_auth() -> Result<()> {
    eprintln!("=== telegram-mcp Authentication ===");
    eprintln!();
    eprintln!("You need a Telegram API ID and API Hash.");
    eprintln!("Get them at: https://my.telegram.org");
    eprintln!("  1. Log in with your phone number");
    eprintln!("  2. Go to 'API development tools'");
    eprintln!("  3. Create an application (any name/description)");
    eprintln!("  4. Copy the api_id (number) and api_hash (hex string)");
    eprintln!();

    let api_id_str = std::env::var("TG_API_ID")
        .unwrap_or_else(|_| rpassword::prompt_password("API ID: ").unwrap());
    let api_id: i32 = api_id_str
        .trim()
        .parse()
        .context("API ID must be a number")?;
    let api_hash = std::env::var("TG_API_HASH")
        .unwrap_or_else(|_| rpassword::prompt_password("API Hash: ").unwrap());
    let api_hash = api_hash.trim().to_string();

    store_config(api_id, &api_hash)?;
    eprintln!("Credentials saved to {}", config_path()?.display());

    // Connect
    let path = session_path()?;
    let path_str = path
        .to_str()
        .context("Session path contains invalid UTF-8")?;
    let session = Arc::new(SqliteSession::open(path_str).await?);

    let SenderPool {
        runner,
        updates: _updates,
        handle,
    } = SenderPool::new(Arc::clone(&session), api_id);

    let client = Client::new(handle.clone());
    let _pool_task = tokio::spawn(async move { runner.run().await });

    if client.is_authorized().await? {
        eprintln!("Already authorized! Session is valid.");
        handle.quit();
        return Ok(());
    }

    // Request login code
    let phone = std::env::var("TG_PHONE").unwrap_or_else(|_| {
        rpassword::prompt_password("Phone number (international format, e.g. +1234567890): ")
            .unwrap()
    });
    let phone = phone.trim().to_string();

    eprintln!("Requesting login code...");
    let token = match client.request_login_code(&phone, &api_hash).await {
        Ok(t) => t,
        Err(e) if format!("{e:?}").contains("AUTH_RESTART") => {
            eprintln!("Auth restart, waiting 10s and retrying...");
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
            client
                .request_login_code(&phone, &api_hash)
                .await
                .context("Failed to request login code")?
        }
        Err(e) => return Err(e).context("Failed to request login code"),
    };

    // Get code: env var > prompt > wait for file
    let code = if let Ok(c) = std::env::var("TG_CODE") {
        c
    } else {
        match rpassword::prompt_password("Login code (check your Telegram app): ") {
            Ok(c) => c,
            Err(_) => {
                // No TTY: wait for code via file
                let code_file = "/tmp/telegram_mcp_code";
                eprintln!(
                    "\nCode sent! Write the code to {code_file} (or set TG_CODE and re-run)."
                );
                eprintln!("Waiting 5 minutes for code...");
                let start = std::time::Instant::now();
                loop {
                    if start.elapsed().as_secs() > 300 {
                        bail!("Timeout waiting for code");
                    }
                    if let Ok(c) = std::fs::read_to_string(code_file) {
                        let c = c.trim().to_string();
                        if !c.is_empty() {
                            let _ = std::fs::remove_file(code_file);
                            break c;
                        }
                    }
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                }
            }
        }
    };
    let code = code.trim().to_string();

    match client.sign_in(&token, &code).await {
        Ok(user) => {
            let name = user.first_name().unwrap_or("(no name)");
            eprintln!("Signed in as {name}!");
        }
        Err(SignInError::PasswordRequired(password_token)) => {
            let hint = password_token.hint().unwrap_or("(no hint)");
            eprintln!("2FA password required. Hint: {hint}");
            let password =
                std::env::var("TG_2FA_PASSWORD").unwrap_or_else(
                    |_| match rpassword::prompt_password("2FA Password: ") {
                        Ok(p) => p,
                        Err(_) => {
                            eprintln!("\nSet TG_2FA_PASSWORD env var to complete login.");
                            std::process::exit(0);
                        }
                    },
                );

            match client
                .check_password(password_token, password.trim().as_bytes())
                .await
            {
                Ok(user) => {
                    let name = user.first_name().unwrap_or("(no name)");
                    eprintln!("Signed in as {name}!");
                }
                Err(SignInError::InvalidPassword(_)) => {
                    bail!("Invalid 2FA password");
                }
                Err(e) => bail!("2FA sign-in failed: {e}"),
            }
        }
        Err(SignInError::InvalidCode) => {
            bail!("Invalid login code. Please try again.");
        }
        Err(SignInError::SignUpRequired) => {
            bail!(
                "This phone number has no Telegram account. Sign up with an official client first."
            );
        }
        Err(e) => bail!("Sign-in failed: {e}"),
    }

    eprintln!("Session saved to {}", path.display());
    eprintln!("You can now use telegram-mcp as an MCP server.");

    handle.quit();
    Ok(())
}
