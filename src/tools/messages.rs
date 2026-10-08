//! Message tools: read, send, schedule, forward, edit, delete and pin
//! messages, inspect/press inline buttons, and list send-as peers.
//!
//! Rust port of `telegram_mcp/tools/messages.py` (chigwell-tg), targeting the
//! grammers 0.10 client API.

use super::{bool_arg, i64_arg, str_arg, usize_arg};
use crate::mcp::{CallToolResult, ToolDefinition};
use anyhow::{Context, bail};
use grammers_client::Client;
use grammers_client::message::{InputMessage, Message};
use grammers_session::types::PeerRef;
use grammers_tl_types as tl;
use serde_json::Value;
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Read an int-ish JSON value (number or numeric string).
fn as_i64(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.trim().parse::<i64>().ok(),
        _ => None,
    }
}

/// Collect message ids from an arg that may be an int or a list of ints.
fn id_list(args: &Value, key: &str) -> Vec<i32> {
    match args.get(key) {
        Some(Value::Array(a)) => a.iter().filter_map(as_i64).map(|i| i as i32).collect(),
        Some(v) => as_i64(v).map(|i| vec![i as i32]).unwrap_or_default(),
        None => Vec::new(),
    }
}

/// Resolve a JSON value holding a numeric chat id or a @username to a PeerRef.
async fn resolve_chat_value(client: &Client, v: &Value) -> anyhow::Result<PeerRef> {
    if let Some(id) = as_i64(v) {
        let mut dialogs = client.iter_dialogs();
        while let Some(d) = dialogs.next().await? {
            if d.peer().id().bare_id() == id {
                return Ok(d.peer_ref());
            }
        }
        bail!("No chat found with id {id}. Use list_chats to find IDs.");
    }
    let name = v.as_str().unwrap_or("").trim();
    let username = name.strip_prefix('@').unwrap_or(name);
    if username.is_empty() {
        bail!("chat id must be a numeric id or a @username");
    }
    let peer = client
        .resolve_username(username)
        .await?
        .with_context(|| format!("No chat found for @{username}"))?;
    peer.to_ref()
        .await
        .with_context(|| format!("No peer reference available for @{username}"))
}

/// Resolve the `chat_id`-style argument `key` (numeric id or @username).
async fn resolve_chat(client: &Client, args: &Value, key: &str) -> anyhow::Result<PeerRef> {
    let v = args.get(key).unwrap_or(&Value::Null);
    resolve_chat_value(client, v)
        .await
        .with_context(|| format!("cannot resolve {key}"))
}

/// Days since the Unix epoch for a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Civil (y, m, d) date from days since the Unix epoch.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Format a unix timestamp as "YYYY-MM-DD HH:MM:SS UTC".
fn fmt_ts(ts: i64) -> String {
    let (y, m, d) = civil_from_days(ts.div_euclid(86400));
    let s = ts.rem_euclid(86400);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC",
        s / 3600,
        (s % 3600) / 60,
        s % 60
    )
}

/// Parse a "YYYY-MM-DD" date arg into the unix timestamp of its start (UTC).
fn parse_day_start(s: &str) -> anyhow::Result<i64> {
    let p: Vec<&str> = s.trim().split('-').collect();
    if p.len() != 3 {
        bail!("date '{s}' must look like YYYY-MM-DD");
    }
    let (y, m, d): (i64, i64, i64) = (p[0].parse()?, p[1].parse()?, p[2].parse()?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        bail!("date '{s}' is not a valid calendar date");
    }
    Ok(days_from_civil(y, m, d) * 86400)
}

/// Parse `schedule_date`: a unix timestamp, or ISO-8601 like
/// "2026-05-01T14:30:00", with optional "Z" or "+HH:MM" suffix.
/// Naive datetimes are treated as UTC.
fn parse_schedule_date(s: &str) -> anyhow::Result<SystemTime> {
    let s = s.trim();
    if let Ok(ts) = s.parse::<i64>() {
        if ts < 0 {
            bail!("schedule_date timestamp must be >= 0");
        }
        return Ok(UNIX_EPOCH + Duration::from_secs(ts as u64));
    }
    let err = || {
        format!("schedule_date '{s}' must be ISO-8601 (2026-05-01T14:30:00) or a unix timestamp")
    };
    let (core, tz_secs): (&str, i64) = if let Some(stripped) = s.strip_suffix(&['Z', 'z'][..]) {
        (stripped, 0)
    } else {
        match s.rfind(&['+', '-'][..]) {
            Some(i) if i > 10 => {
                let digits: String = s[i + 1..].chars().filter(|c| *c != ':').collect();
                if digits.len() == 4 && digits.chars().all(|c| c.is_ascii_digit()) {
                    let sign = if s.as_bytes()[i] == b'-' { -1 } else { 1 };
                    let hh: i64 = digits[..2].parse()?;
                    let mm: i64 = digits[2..].parse()?;
                    (&s[..i], sign * (hh * 3600 + mm * 60))
                } else {
                    (s, 0)
                }
            }
            _ => (s, 0),
        }
    };
    let mut parts = core.split(&['T', ' '][..]);
    let date = parts.next().unwrap_or("");
    let time = parts.next().unwrap_or("00:00:00");
    if parts.next().is_some() {
        bail!("{}", err());
    }
    let dp: Vec<&str> = date.split('-').collect();
    if dp.len() != 3 {
        bail!("{}", err());
    }
    let (y, mo, d): (i64, i64, i64) = (dp[0].parse()?, dp[1].parse()?, dp[2].parse()?);
    let tp: Vec<&str> = time.split(':').collect();
    if tp.len() < 2 || tp.len() > 3 {
        bail!("{}", err());
    }
    let hh: i64 = tp[0].parse()?;
    let mm: i64 = tp[1].parse()?;
    let ss: i64 = if tp.len() == 3 {
        tp[2].split('.').next().unwrap_or("0").parse()?
    } else {
        0
    };
    let ts = days_from_civil(y, mo, d) * 86400 + hh * 3600 + mm * 60 + ss - tz_secs;
    if ts < 0 {
        bail!("schedule_date '{s}' is before the Unix epoch");
    }
    Ok(UNIX_EPOCH + Duration::from_secs(ts as u64))
}

/// Reject rich/premium-only formatting modes.
///
/// md/html are accepted for API compatibility but sent as plain text: this
/// build of grammers-client has no `markdown`/`html` parser features enabled.
fn check_parse_mode(args: &Value) -> anyhow::Result<()> {
    let mode = args
        .get("parse_mode")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase();
    match mode.as_str() {
        "" | "plain" | "md" | "markdown" | "html" => Ok(()),
        "rich" | "rich_md" | "rich_markdown" | "rich_html" => {
            bail!("not supported: rich formatting requires Telegram Premium on the account")
        }
        other => bail!("unknown parse_mode '{other}'; use md, html or plain"),
    }
}

/// Single-line human-readable message representation.
fn fmt_message_line(msg: &Message) -> String {
    let date = msg.date().format("%Y-%m-%d %H:%M");
    let sender = msg
        .sender()
        .and_then(|p| p.name().map(str::to_string))
        .unwrap_or_else(|| "?".into());
    let text: String = msg.text().chars().take(200).collect();
    let text = text.replace('\n', " ");
    let mut line = format!("- id:{} | {sender} | {date}", msg.id());
    if msg.pinned() {
        line.push_str(" | pinned");
    }
    if msg.edit_date().is_some() {
        line.push_str(" | edited");
    }
    if let Some(gid) = msg.grouped_id() {
        line.push_str(&format!(" | album:{gid}"));
    }
    if msg.action().is_some() {
        line.push_str(" | service");
    }
    line.push_str(&format!(" | {text}"));
    line
}

/// A flattened inline button: text, optional callback payload, optional URL.
struct InlineButton {
    text: String,
    data: Option<Vec<u8>>,
    url: Option<String>,
}

/// Extract inline buttons from a message's reply markup.
fn inline_buttons(msg: &Message) -> Vec<InlineButton> {
    let mut out = Vec::new();
    let Some(tl::enums::ReplyMarkup::ReplyInlineMarkup(markup)) = msg.reply_markup() else {
        return out;
    };
    for row in markup.rows {
        let tl::enums::KeyboardButtonRow::Row(row) = row;
        for btn in row.buttons {
            let b = match btn {
                tl::enums::KeyboardButton::Callback(b) => InlineButton {
                    text: b.text,
                    data: Some(b.data),
                    url: None,
                },
                tl::enums::KeyboardButton::Url(b) => InlineButton {
                    text: b.text,
                    data: None,
                    url: Some(b.url),
                },
                tl::enums::KeyboardButton::UrlAuth(b) => InlineButton {
                    text: b.text,
                    data: None,
                    url: Some(b.url),
                },
                _ => continue,
            };
            out.push(b);
        }
    }
    out
}

/// Tiny xorshift RNG for Telegram `random_id` fields (no rand crate here).
fn gen_random_ids(n: usize) -> Vec<i64> {
    let mut x = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9e3779b97f4a7c15)
        | 1;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as i64
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tool implementations (each returns the human-readable result text)
// ---------------------------------------------------------------------------

async fn get_messages(client: &Client, args: &Value) -> anyhow::Result<String> {
    let peer = resolve_chat(client, args, "chat_id").await?;
    let page = usize_arg(args, "page", 1).max(1);
    let page_size = usize_arg(args, "page_size", 20).clamp(1, 100);
    let offset = (page - 1) * page_size;

    let mut iter = client.iter_messages(peer);
    let mut out = String::new();
    let mut skipped = 0;
    let mut taken = 0;
    while let Some(msg) = iter.next().await? {
        if skipped < offset {
            skipped += 1;
            continue;
        }
        out.push_str(&fmt_message_line(&msg));
        out.push('\n');
        taken += 1;
        if taken >= page_size {
            break;
        }
    }
    if out.is_empty() {
        out = "No messages found for this page.".into();
    }
    Ok(out)
}

async fn send_message(client: &Client, args: &Value) -> anyhow::Result<String> {
    let peer = resolve_chat(client, args, "chat_id").await?;
    let message = str_arg(args, "message");
    if message.is_empty() {
        bail!("message must not be empty");
    }
    check_parse_mode(args)?;
    let reply_to = args.get("reply_to_message_id").and_then(|v| v.as_i64()).map(|v| v as i32);
    let sent = if let Some(reply_id) = reply_to {
        let input = InputMessage::new().text(message).reply_to(Some(reply_id));
        client.send_message(peer, input).await?
    } else {
        client.send_message(peer, message).await?
    };
    Ok(format!("Message sent successfully (id {}).", sent.id()))
}

async fn send_scheduled_message(client: &Client, args: &Value) -> anyhow::Result<String> {
    let peer = resolve_chat(client, args, "chat_id").await?;
    let message = str_arg(args, "message");
    if message.is_empty() {
        bail!("message must not be empty");
    }
    check_parse_mode(args)?;
    let when = match args.get("schedule_date") {
        Some(Value::Number(n)) => {
            let ts = n
                .as_i64()
                .with_context(|| "schedule_date must be ISO-8601 or a unix timestamp")?;
            if ts < 0 {
                bail!("schedule_date timestamp must be >= 0");
            }
            UNIX_EPOCH + Duration::from_secs(ts as u64)
        }
        Some(Value::String(s)) => parse_schedule_date(s)?,
        _ => bail!("schedule_date is required (ISO-8601 string or unix timestamp)"),
    };
    let input = InputMessage::new().text(message).schedule_date(Some(when));
    let sent = client.send_message(peer, input).await?;
    let ts = when
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    Ok(format!(
        "Scheduled message {} for {}.",
        sent.id(),
        fmt_ts(ts)
    ))
}

async fn get_scheduled_messages(client: &Client, args: &Value) -> anyhow::Result<String> {
    let peer = resolve_chat(client, args, "chat_id").await?;
    let res = client
        .invoke(&tl::functions::messages::GetScheduledHistory {
            peer: peer.into(),
            hash: 0,
        })
        .await?;
    let messages = match res {
        tl::enums::messages::Messages::Messages(m) => m.messages,
        tl::enums::messages::Messages::Slice(m) => m.messages,
        tl::enums::messages::Messages::ChannelMessages(m) => m.messages,
        tl::enums::messages::Messages::NotModified(_) => {
            bail!("unexpected NotModified response from Telegram")
        }
    };
    if messages.is_empty() {
        return Ok("No scheduled messages in this chat.".into());
    }
    let mut out = format!("Scheduled messages ({}):\n", messages.len());
    for m in &messages {
        if let tl::enums::Message::Message(m) = m {
            let preview: String = m.message.chars().take(100).collect();
            out.push_str(&format!(
                "- id:{} | scheduled:{} | {}\n",
                m.id,
                fmt_ts(m.date as i64),
                preview.replace('\n', " ")
            ));
        }
    }
    Ok(out)
}

async fn delete_scheduled_message(client: &Client, args: &Value) -> anyhow::Result<String> {
    let peer = resolve_chat(client, args, "chat_id").await?;
    let ids = id_list(args, "message_ids");
    if ids.is_empty() {
        bail!("message_ids must be a non-empty list of message ids");
    }
    let n = ids.len();
    client
        .invoke(&tl::functions::messages::DeleteScheduledMessages {
            peer: peer.into(),
            id: ids,
        })
        .await?;
    Ok(format!("Deleted {n} scheduled message(s)."))
}

async fn list_inline_buttons(client: &Client, args: &Value) -> anyhow::Result<String> {
    let peer = resolve_chat(client, args, "chat_id").await?;
    let limit = usize_arg(args, "limit", 20).clamp(1, 100);

    let target: Option<Message> = match as_i64(args.get("message_id").unwrap_or(&Value::Null)) {
        Some(id) => {
            let mut v = client.get_messages_by_id(peer, &[id as i32]).await?;
            v.pop().flatten()
        }
        None => {
            let mut iter = client.iter_messages(peer);
            let mut found = None;
            let mut n = 0;
            while let Some(m) = iter.next().await? {
                if !inline_buttons(&m).is_empty() {
                    found = Some(m);
                    break;
                }
                n += 1;
                if n >= limit {
                    break;
                }
            }
            found
        }
    };
    let msg = target.with_context(|| "No message with inline buttons found.")?;
    let buttons = inline_buttons(&msg);
    if buttons.is_empty() {
        bail!("Message {} has no inline buttons.", msg.id());
    }
    let mut out = format!(
        "Inline buttons on message {} ({} buttons):\n",
        msg.id(),
        buttons.len()
    );
    for (i, b) in buttons.iter().enumerate() {
        let kind = if b.data.is_some() {
            "callback"
        } else if b.url.is_some() {
            "url"
        } else {
            "other"
        };
        let extra = b.url.as_deref().unwrap_or("");
        out.push_str(&format!("- [{i}] {} [{kind}] {extra}\n", b.text.trim()));
    }
    Ok(out)
}

async fn press_inline_button(client: &Client, args: &Value) -> anyhow::Result<String> {
    let peer = resolve_chat(client, args, "chat_id").await?;
    let button_text = args.get("button_text").and_then(Value::as_str);
    let button_index = as_i64(args.get("button_index").unwrap_or(&Value::Null));
    if button_text.is_none() && button_index.is_none() {
        bail!("Provide button_text or button_index to choose a button.");
    }

    let target: Option<Message> = match as_i64(args.get("message_id").unwrap_or(&Value::Null)) {
        Some(id) => {
            let mut v = client.get_messages_by_id(peer, &[id as i32]).await?;
            v.pop().flatten()
        }
        None => {
            let mut iter = client.iter_messages(peer);
            let mut found = None;
            for _ in 0..20 {
                match iter.next().await? {
                    Some(m) if !inline_buttons(&m).is_empty() => {
                        found = Some(m);
                        break;
                    }
                    Some(_) => continue,
                    None => break,
                }
            }
            found
        }
    };
    let msg = target.with_context(|| "No message with inline buttons found.")?;
    let buttons = inline_buttons(&msg);
    if buttons.is_empty() {
        bail!("Message {} has no inline buttons.", msg.id());
    }
    let available: Vec<String> = buttons
        .iter()
        .enumerate()
        .map(|(i, b)| format!("[{i}] {}", b.text.trim()))
        .collect();
    let btn = if let Some(t) = button_text {
        buttons
            .iter()
            .find(|b| b.text.trim().eq_ignore_ascii_case(t.trim()))
            .with_context(|| {
                format!(
                    "Button '{t}' not found. Available: {}",
                    available.join(", ")
                )
            })?
    } else {
        let i = button_index.unwrap_or(-1);
        buttons.get(i as usize).with_context(|| {
            format!(
                "button_index out of range. Valid indices: 0-{}.",
                buttons.len() - 1
            )
        })?
    };

    match (&btn.data, &btn.url) {
        (Some(data), _) => {
            let res = client
                .invoke(&tl::functions::messages::GetBotCallbackAnswer {
                    game: false,
                    peer: peer.into(),
                    msg_id: msg.id(),
                    data: Some(data.clone()),
                    password: None,
                })
                .await?;
            let tl::enums::messages::BotCallbackAnswer::Answer(a) = res;
            let mut parts = Vec::new();
            if let Some(m) = a.message {
                parts.push(m);
            }
            if let Some(u) = a.url {
                parts.push(format!("url: {u}"));
            }
            if a.alert {
                parts.push("(Telegram displayed an alert)".into());
            }
            if parts.is_empty() {
                parts.push("Button pressed successfully.".into());
            }
            Ok(parts.join(" "))
        }
        (None, Some(url)) => Ok(format!(
            "Selected button opens a URL instead of sending a callback: {url}"
        )),
        (None, None) => bail!("Selected button provides no callback data to press."),
    }
}

async fn list_messages(client: &Client, args: &Value) -> anyhow::Result<String> {
    let peer = resolve_chat(client, args, "chat_id").await?;
    let limit = usize_arg(args, "limit", 20).clamp(1, 100);
    let from_ts = args
        .get("from_date")
        .and_then(Value::as_str)
        .map(parse_day_start)
        .transpose()?;
    // to_date is inclusive: end of that day.
    let to_ts = args
        .get("to_date")
        .and_then(Value::as_str)
        .map(|s| parse_day_start(s).map(|t| t + 86399))
        .transpose()?;

    let mut out = String::new();
    let mut count = 0;
    let in_range = |m: &Message| {
        let ts = m.date().timestamp();
        from_ts.is_none_or(|lo| ts >= lo) && to_ts.is_none_or(|hi| ts <= hi)
    };

    match args.get("search_query").and_then(Value::as_str) {
        Some(q) if !q.is_empty() => {
            let mut search = client.search_messages(peer).query(q);
            while let Some(m) = search.next().await? {
                if !in_range(&m) {
                    continue;
                }
                out.push_str(&fmt_message_line(&m));
                out.push('\n');
                count += 1;
                if count >= limit {
                    break;
                }
            }
        }
        _ => {
            let mut iter = client.iter_messages(peer);
            while let Some(m) = iter.next().await? {
                if !in_range(&m) {
                    continue;
                }
                out.push_str(&fmt_message_line(&m));
                out.push('\n');
                count += 1;
                if count >= limit {
                    break;
                }
            }
        }
    }
    if out.is_empty() {
        out = "No messages found matching the criteria.".into();
    }
    Ok(out)
}

async fn transcribe_voice(_client: &Client, _args: &Value) -> anyhow::Result<String> {
    // TODO: grammers has no speech-to-text. Implement by downloading the voice
    // note via Client::download_media and feeding it to an external engine
    // (local whisper / Groq / OpenAI-compatible endpoint), or by invoking
    // messages.transcribeAudio for native Telegram Premium transcription.
    bail!("not supported: transcribe_voice needs an external speech-to-text engine")
}

async fn get_message_context(client: &Client, args: &Value) -> anyhow::Result<String> {
    let peer = resolve_chat(client, args, "chat_id").await?;
    let message_id = i64_arg(args, "message_id", 0) as i32;
    if message_id <= 0 {
        bail!("message_id is required");
    }
    let size = usize_arg(args, "context_size", 3).clamp(0, 50) as i32;

    let ids: Vec<i32> = ((message_id - size).max(1)..=message_id + size).collect();
    let mut found: Vec<Message> = client
        .get_messages_by_id(peer, &ids)
        .await?
        .into_iter()
        .flatten()
        .collect();
    if !found.iter().any(|m| m.id() == message_id) {
        bail!("Message {message_id} not found in this chat.");
    }
    found.sort_by_key(|m| m.id());

    let mut out = String::new();
    for m in &found {
        let marker = if m.id() == message_id { ">>>" } else { "   " };
        out.push_str(marker);
        out.push_str(&fmt_message_line(m));
        out.push('\n');
    }
    Ok(out)
}

async fn get_send_as(client: &Client, args: &Value) -> anyhow::Result<String> {
    let peer = resolve_chat(client, args, "chat_id").await?;
    let res = client
        .invoke(&tl::functions::channels::GetSendAs {
            for_paid_reactions: false,
            for_live_stories: false,
            peer: peer.into(),
        })
        .await?;
    let tl::enums::channels::SendAsPeers::Peers(p) = res;

    let mut user_names: HashMap<i64, String> = HashMap::new();
    for u in &p.users {
        if let tl::enums::User::User(u) = u {
            let mut name = u.first_name.clone().unwrap_or_default();
            if let Some(last) = &u.last_name {
                if !name.is_empty() {
                    name.push(' ');
                }
                name.push_str(last);
            }
            user_names.insert(u.id, name);
        }
    }
    let mut chat_names: HashMap<i64, String> = HashMap::new();
    for c in &p.chats {
        match c {
            tl::enums::Chat::Chat(c) => {
                chat_names.insert(c.id, c.title.clone());
            }
            tl::enums::Chat::Channel(c) => {
                chat_names.insert(c.id, c.title.clone());
            }
            _ => {}
        }
    }

    if p.peers.is_empty() {
        return Ok("No alternative send-as peers available for this chat.".into());
    }
    let mut out = format!("Allowed send-as peers ({}):\n", p.peers.len());
    for sp in &p.peers {
        let tl::enums::SendAsPeer::Peer(sp) = sp;
        let (id, kind, name) = match &sp.peer {
            tl::enums::Peer::User(u) => (
                u.user_id,
                "user",
                user_names.get(&u.user_id).cloned().unwrap_or_default(),
            ),
            tl::enums::Peer::Chat(c) => (
                c.chat_id,
                "group",
                chat_names.get(&c.chat_id).cloned().unwrap_or_default(),
            ),
            tl::enums::Peer::Channel(c) => (
                c.channel_id,
                "channel",
                chat_names.get(&c.channel_id).cloned().unwrap_or_default(),
            ),
        };
        out.push_str(&format!(
            "- id:{id} | {kind} | {name} | premium_required:{}\n",
            sp.premium_required
        ));
    }
    Ok(out)
}

async fn forward_message(client: &Client, args: &Value) -> anyhow::Result<String> {
    let from_peer = resolve_chat(client, args, "from_chat_id").await?;
    let to_peer = resolve_chat(client, args, "to_chat_id").await?;
    let mut ids = id_list(args, "message_id");
    if ids.is_empty() {
        bail!("message_id must be an int or a non-empty list of ints");
    }

    // Auto-expand a single message to its full album when applicable.
    let mut expanded = false;
    if bool_arg(args, "expand_album", true) && ids.len() == 1 {
        let anchor = ids[0];
        let got = client.get_messages_by_id(from_peer, &[anchor]).await?;
        if let Some(m) = got.into_iter().flatten().next()
            && let Some(gid) = m.grouped_id()
        {
            let window: Vec<i32> = ((anchor - 9).max(1)..=anchor + 10).collect();
            let neighbors = client.get_messages_by_id(from_peer, &window).await?;
            let mut sibs: Vec<i32> = neighbors
                .into_iter()
                .flatten()
                .filter(|n| n.grouped_id() == Some(gid))
                .map(|n| n.id())
                .collect();
            sibs.sort_unstable();
            sibs.dedup();
            if sibs.len() > 1 {
                ids = sibs;
                expanded = true;
            }
        }
    }

    let topic_id = as_i64(args.get("topic_id").unwrap_or(&Value::Null)).map(|t| t as i32);
    if let Some(t) = topic_id
        && t <= 0
    {
        bail!("topic_id must be a positive integer");
    }
    let send_as: Option<PeerRef> = match args.get("send_as") {
        Some(v) if !matches!(v, Value::Null) => Some(resolve_chat_value(client, v).await?),
        _ => None,
    };
    let drop_author = bool_arg(args, "drop_author", false);
    let silent = bool_arg(args, "silent", false);

    if topic_id.is_some() || send_as.is_some() || drop_author || silent {
        let send_as_input: Option<tl::enums::InputPeer> = send_as.map(|p| p.into());
        client
            .invoke(&tl::functions::messages::ForwardMessages {
                silent,
                background: false,
                with_my_score: false,
                drop_author,
                drop_media_captions: false,
                from_peer: from_peer.into(),
                id: ids.clone(),
                random_id: gen_random_ids(ids.len()),
                to_peer: to_peer.into(),
                top_msg_id: topic_id,
                reply_to: None,
                schedule_date: None,
                schedule_repeat_period: None,
                send_as: send_as_input,
                noforwards: false,
                allow_paid_floodskip: false,
                quick_reply_shortcut: None,
                effect: None,
                video_timestamp: None,
                allow_paid_stars: None,
                suggested_post: None,
            })
            .await?;
        return Ok(format!(
            "Forwarded {} message(s) with custom send options.",
            ids.len()
        ));
    }

    let fwd = client.forward_messages(to_peer, &ids, from_peer).await?;
    let n = fwd.iter().filter(|m| m.is_some()).count();
    let summary = if expanded {
        format!(
            "Album of {n} messages forwarded (auto-expanded from message {}).",
            ids[0]
        )
    } else if n == 1 {
        format!("Message {} forwarded.", ids[0])
    } else {
        format!("{n} messages forwarded.")
    };
    Ok(summary)
}

async fn forward_messages(client: &Client, args: &Value) -> anyhow::Result<String> {
    let from_peer = resolve_chat(client, args, "from_chat_id").await?;
    let to_peer = resolve_chat(client, args, "to_chat_id").await?;
    let ids = id_list(args, "message_ids");
    if ids.is_empty() {
        bail!("message_ids must contain at least one id");
    }
    let fwd = client.forward_messages(to_peer, &ids, from_peer).await?;
    let n = fwd.iter().filter(|m| m.is_some()).count();
    Ok(format!("{n} of {} messages forwarded.", ids.len()))
}

async fn edit_message(client: &Client, args: &Value) -> anyhow::Result<String> {
    let peer = resolve_chat(client, args, "chat_id").await?;
    let message_id = i64_arg(args, "message_id", 0) as i32;
    if message_id <= 0 {
        bail!("message_id is required");
    }
    let new_text = str_arg(args, "new_text");
    if new_text.is_empty() {
        bail!("new_text must not be empty");
    }
    check_parse_mode(args)?;
    client.edit_message(peer, message_id, new_text).await?;
    Ok(format!("Message {message_id} edited."))
}

async fn delete_message(client: &Client, args: &Value) -> anyhow::Result<String> {
    let peer = resolve_chat(client, args, "chat_id").await?;
    let message_id = i64_arg(args, "message_id", 0) as i32;
    if message_id <= 0 {
        bail!("message_id is required");
    }
    client.delete_messages(peer, &[message_id]).await?;
    Ok(format!("Message {message_id} deleted."))
}

async fn delete_chat_history(client: &Client, args: &Value) -> anyhow::Result<String> {
    let peer = resolve_chat(client, args, "chat_id").await?;
    let max_id = i64_arg(args, "max_id", 0) as i32;
    let revoke = bool_arg(args, "revoke", false);
    let res = client
        .invoke(&tl::functions::messages::DeleteHistory {
            just_clear: false,
            revoke,
            peer: peer.into(),
            max_id,
            min_date: None,
            max_date: None,
        })
        .await?;
    let tl::enums::messages::AffectedHistory::History(a) = res;
    let scope = if revoke {
        "for both parties"
    } else {
        "for you"
    };
    Ok(format!(
        "Chat history cleared {scope}: {} messages deleted.",
        a.pts_count
    ))
}

async fn delete_messages_bulk(client: &Client, args: &Value) -> anyhow::Result<String> {
    let peer = resolve_chat(client, args, "chat_id").await?;
    let ids = id_list(args, "message_ids");
    if ids.is_empty() {
        bail!("message_ids must be a non-empty list of message ids");
    }
    // grammers always revokes non-channel deletes; the `revoke` arg is kept
    // for API compatibility with the Python version.
    let n = client.delete_messages(peer, &ids).await?;
    Ok(format!("Deleted {n} of {} messages.", ids.len()))
}

async fn pin_message(client: &Client, args: &Value) -> anyhow::Result<String> {
    let peer = resolve_chat(client, args, "chat_id").await?;
    let message_id = i64_arg(args, "message_id", 0) as i32;
    if message_id <= 0 {
        bail!("message_id is required");
    }
    client.pin_message(peer, message_id).await?;
    Ok(format!("Message {message_id} pinned."))
}

// ---------------------------------------------------------------------------
// Tool definitions and dispatch
// ---------------------------------------------------------------------------

fn def(name: &str, description: &str, input_schema: Value) -> ToolDefinition {
    ToolDefinition {
        name: name.into(),
        description: description.into(),
        input_schema,
    }
}

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        def(
            "get_messages",
            "Get paginated messages from a chat, newest first.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "chat_id": { "description": "Numeric chat ID or @username" },
                    "page": { "type": "integer", "description": "Page number, 1-indexed (default: 1)", "default": 1 },
                    "page_size": { "type": "integer", "description": "Messages per page (default: 20)", "default": 20 }
                },
                "required": ["chat_id"]
            }),
        ),
        def(
            "send_message",
            "Send a message to a chat. parse_mode md/html are accepted but sent as plain text in this build.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "chat_id": { "description": "Numeric chat ID or @username" },
                    "message": { "type": "string", "description": "Message text" },
                    "parse_mode": { "type": "string", "description": "md, html or plain (default plain)" },
                    "reply_to_message_id": { "type": "integer", "description": "Reply to (quote) a specific message ID" }
                },
                "required": ["chat_id", "message"]
            }),
        ),
        def(
            "send_scheduled_message",
            "Schedule a message to be sent at a future time.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "chat_id": { "description": "Numeric chat ID or @username" },
                    "message": { "type": "string", "description": "Message text" },
                    "schedule_date": { "description": "When to send: ISO-8601 (e.g. 2026-05-01T14:30:00) or unix timestamp. Naive datetimes are UTC." },
                    "parse_mode": { "type": "string", "description": "md, html or plain (default plain)" }
                },
                "required": ["chat_id", "message", "schedule_date"]
            }),
        ),
        def(
            "get_scheduled_messages",
            "List all scheduled (pending) messages in a chat.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "chat_id": { "description": "Numeric chat ID or @username" }
                },
                "required": ["chat_id"]
            }),
        ),
        def(
            "delete_scheduled_message",
            "Delete one or more scheduled (pending) messages from a chat.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "chat_id": { "description": "Numeric chat ID or @username" },
                    "message_ids": { "type": "array", "items": { "type": "integer" }, "description": "Scheduled message IDs to delete" }
                },
                "required": ["chat_id", "message_ids"]
            }),
        ),
        def(
            "list_inline_buttons",
            "Inspect inline buttons on a message to discover their indices, text and URLs. Without message_id, scans recent messages.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "chat_id": { "description": "Numeric chat ID or @username" },
                    "message_id": { "type": "integer", "description": "Specific message to inspect" },
                    "limit": { "type": "integer", "description": "How many recent messages to scan (default: 20)", "default": 20 }
                },
                "required": ["chat_id"]
            }),
        ),
        def(
            "press_inline_button",
            "Press an inline button (callback) on a chat message.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "chat_id": { "description": "Numeric chat ID or @username" },
                    "message_id": { "type": "integer", "description": "Message holding the button (omit to search recent messages)" },
                    "button_text": { "type": "string", "description": "Exact button text (case-insensitive)" },
                    "button_index": { "type": "integer", "description": "Zero-based index among the message's buttons" }
                },
                "required": ["chat_id"]
            }),
        ),
        def(
            "list_messages",
            "Retrieve messages with optional text search and date filters (YYYY-MM-DD).",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "chat_id": { "description": "Numeric chat ID or @username" },
                    "limit": { "type": "integer", "description": "Max messages (default: 20)", "default": 20 },
                    "search_query": { "type": "string", "description": "Only messages containing this text" },
                    "from_date": { "type": "string", "description": "Only messages from this date (YYYY-MM-DD)" },
                    "to_date": { "type": "string", "description": "Only messages up to this date, inclusive (YYYY-MM-DD)" }
                },
                "required": ["chat_id"]
            }),
        ),
        def(
            "transcribe_voice",
            "Transcribe a voice message or video note to text. NOT SUPPORTED: needs an external speech-to-text engine.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "chat_id": { "description": "Numeric chat ID or @username" },
                    "message_id": { "type": "integer", "description": "Message holding the voice/video-note media" },
                    "engine": { "type": "string", "description": "groq, telegram, openai or whisper" }
                },
                "required": ["chat_id", "message_id"]
            }),
        ),
        def(
            "get_message_context",
            "Retrieve messages around a specific message ID, with the target marked.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "chat_id": { "description": "Numeric chat ID or @username" },
                    "message_id": { "type": "integer", "description": "Central message ID" },
                    "context_size": { "type": "integer", "description": "Messages before and after (default: 3)", "default": 3 }
                },
                "required": ["chat_id", "message_id"]
            }),
        ),
        def(
            "get_send_as",
            "List Telegram's allowed send-as peers for a chat (peer IDs, names, premium_required).",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "chat_id": { "description": "Numeric chat ID or @username" }
                },
                "required": ["chat_id"]
            }),
        ),
        def(
            "forward_message",
            "Forward message(s) from one chat to another. A single message_id auto-expands to the full album.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "from_chat_id": { "description": "Source chat (id or @username)" },
                    "message_id": { "description": "Single message id or list of ids" },
                    "to_chat_id": { "description": "Destination chat (id or @username)" },
                    "expand_album": { "type": "boolean", "description": "Auto-expand single id to full album (default: true)", "default": true },
                    "topic_id": { "type": "integer", "description": "Forum topic ID (top_msg_id) where supported" },
                    "send_as": { "description": "Sender ID or username (see get_send_as)" },
                    "drop_author": { "type": "boolean", "description": "Hide forward attribution (default: false)", "default": false },
                    "silent": { "type": "boolean", "description": "Send without notification (default: false)", "default": false }
                },
                "required": ["from_chat_id", "message_id", "to_chat_id"]
            }),
        ),
        def(
            "forward_messages",
            "Forward a batch of messages from one chat to another in a single atomic call.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "from_chat_id": { "description": "Source chat (id or @username)" },
                    "message_ids": { "type": "array", "items": { "type": "integer" }, "description": "Message IDs to forward" },
                    "to_chat_id": { "description": "Destination chat (id or @username)" }
                },
                "required": ["from_chat_id", "message_ids", "to_chat_id"]
            }),
        ),
        def(
            "edit_message",
            "Edit a message you sent.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "chat_id": { "description": "Numeric chat ID or @username" },
                    "message_id": { "type": "integer", "description": "Message ID to edit" },
                    "new_text": { "type": "string", "description": "Replacement text" },
                    "parse_mode": { "type": "string", "description": "md, html or plain (default plain)" }
                },
                "required": ["chat_id", "message_id", "new_text"]
            }),
        ),
        def(
            "delete_message",
            "Delete a message by ID.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "chat_id": { "description": "Numeric chat ID or @username" },
                    "message_id": { "type": "integer", "description": "Message ID to delete" }
                },
                "required": ["chat_id", "message_id"]
            }),
        ),
        def(
            "delete_chat_history",
            "Clear the message history of a chat.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "chat_id": { "description": "Numeric chat ID or @username" },
                    "max_id": { "type": "integer", "description": "Delete up to this ID; 0 deletes all (default: 0)", "default": 0 },
                    "revoke": { "type": "boolean", "description": "Delete for both parties (default: false)", "default": false }
                },
                "required": ["chat_id"]
            }),
        ),
        def(
            "delete_messages_bulk",
            "Delete multiple messages in a single call.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "chat_id": { "description": "Numeric chat ID or @username" },
                    "message_ids": { "type": "array", "items": { "type": "integer" }, "description": "Message IDs to delete" },
                    "revoke": { "type": "boolean", "description": "Kept for API compatibility (default: true)", "default": true }
                },
                "required": ["chat_id", "message_ids"]
            }),
        ),
        def(
            "pin_message",
            "Pin a message in a chat.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "chat_id": { "description": "Numeric chat ID or @username" },
                    "message_id": { "type": "integer", "description": "Message ID to pin" }
                },
                "required": ["chat_id", "message_id"]
            }),
        ),
    ]
}

pub async fn try_handle(
    client: &Client,
    tool_name: &str,
    args: &Value,
) -> anyhow::Result<Option<CallToolResult>> {
    let text = match tool_name {
        "get_messages" => get_messages(client, args).await?,
        "send_message" => send_message(client, args).await?,
        "send_scheduled_message" => send_scheduled_message(client, args).await?,
        "get_scheduled_messages" => get_scheduled_messages(client, args).await?,
        "delete_scheduled_message" => delete_scheduled_message(client, args).await?,
        "list_inline_buttons" => list_inline_buttons(client, args).await?,
        "press_inline_button" => press_inline_button(client, args).await?,
        "list_messages" => list_messages(client, args).await?,
        "transcribe_voice" => transcribe_voice(client, args).await?,
        "get_message_context" => get_message_context(client, args).await?,
        "get_send_as" => get_send_as(client, args).await?,
        "forward_message" => forward_message(client, args).await?,
        "forward_messages" => forward_messages(client, args).await?,
        "edit_message" => edit_message(client, args).await?,
        "delete_message" => delete_message(client, args).await?,
        "delete_chat_history" => delete_chat_history(client, args).await?,
        "delete_messages_bulk" => delete_messages_bulk(client, args).await?,
        "pin_message" => pin_message(client, args).await?,
        _ => return Ok(None),
    };
    Ok(Some(CallToolResult::ok(text)))
}
