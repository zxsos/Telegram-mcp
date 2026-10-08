//! Telegram API helpers.

use anyhow::{Context, Result};
use grammers_client::Client;
use grammers_session::types::PeerRef;

/// Resolve a "@username" or "username" to a PeerRef.
pub async fn resolve(client: &Client, channel: &str) -> Result<PeerRef> {
    let username = channel.strip_prefix('@').unwrap_or(channel);
    let peer = client
        .resolve_username(username)
        .await
        .context("Failed to resolve username")?
        .with_context(|| format!("No channel/user found for @{username}"))?;
    peer.to_ref().await.context("Failed to get peer ref")
}
