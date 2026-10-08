//! Media MCP tools (Rust port of `telegram_mcp/tools/media.py`).
//!
//! Deliberate degradations vs the Python original:
//! - This crate's [`CallToolResult`] only supports text content, so
//!   `open_photo`, `get_photo_sheet` and `inspect_document` save image bytes
//!   to a temp file and return the file path instead of MCP image content.
//! - `send_gif` is not implemented: grammers 0.10 cannot resolve a bare
//!   document id into the `(id, access_hash, file_reference)` triple needed
//!   to send it (see TODO in the handler).
//! - `get_photo_sheet` returns individual thumbnail files: composing a
//!   labelled contact sheet would need an image-processing crate.
//! - PDF text extraction is not implemented (would need a PDF parsing crate);
//!   PDFs are saved to a temp file and their path is returned.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use grammers_client::Client;
use grammers_client::media::{Attribute, Downloadable, InputMedia, Media, Uploaded};
use grammers_client::message::{InputMessage, Message};
use grammers_session::types::{PeerKind, PeerRef};
use grammers_tl_types as tl;
use serde_json::{Value, json};

use super::{i64_arg, str_arg, usize_arg};
use crate::mcp::{CallToolResult, ToolDefinition};

/// Refuse downloads larger than this (mirrors the Python size guard).
const MAX_DOWNLOAD_BYTES: usize = 200 * 1024 * 1024;
/// Cap for text returned by `inspect_document`.
const MAX_TEXT_CHARS: usize = 100_000;

// ── helpers ──────────────────────────────────────────────────────────────

/// Resolve a `chat_id` argument (numeric id or `@username`) to a peer.
async fn resolve_chat(client: &Client, chat_id: &Value) -> Result<PeerRef> {
    if let Some(id) = chat_id.as_i64() {
        return resolve_chat_by_id(client, id).await;
    }
    let name = chat_id
        .as_str()
        .unwrap_or("")
        .trim()
        .trim_start_matches('@');
    if name.is_empty() {
        bail!("chat_id is required");
    }
    if let Ok(id) = name.parse::<i64>() {
        return resolve_chat_by_id(client, id).await;
    }
    let peer = client
        .resolve_username(name)
        .await
        .context("failed to resolve username")?
        .with_context(|| format!("no chat found for @{name}"))?;
    peer.to_ref().await.context("failed to get peer ref")
}

async fn resolve_chat_by_id(client: &Client, id: i64) -> Result<PeerRef> {
    let mut dialogs = client.iter_dialogs();
    while let Some(dialog) = dialogs.next().await.context("failed to list dialogs")? {
        if dialog.peer().id().bare_id() == id {
            return Ok(dialog.peer_ref());
        }
    }
    bail!("no chat found with id {id} (use list_chats to find ids)")
}

/// Fetch a single message by id.
async fn get_message(client: &Client, peer: PeerRef, message_id: i32) -> Result<Message> {
    let messages = client
        .get_messages_by_id(peer, &[message_id])
        .await
        .context("failed to fetch message")?;
    messages
        .into_iter()
        .next()
        .flatten()
        .with_context(|| format!("no message with id {message_id}"))
}

/// Unique temp file path.
fn temp_path(prefix: &str, ext: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "{prefix}-{nanos:x}-{}.{}",
        std::process::id(),
        ext.trim_start_matches('.')
    ))
}

fn chat_label(chat_id: &Value) -> String {
    match chat_id {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        _ => "(unknown)".into(),
    }
}

fn is_photo_ext(path: &str) -> bool {
    matches!(
        path.rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "jpg" | "jpeg" | "png" | "bmp" | "tif" | "tiff"
    )
}

/// Parse `schedule_date`: unix timestamp (int or numeric string) or
/// ISO-8601 `YYYY-MM-DDTHH:MM:SS` with optional `Z` / `±HH:MM` offset.
/// Naive datetimes are treated as UTC.
fn parse_schedule_date(raw: &Value) -> Result<Option<SystemTime>> {
    if raw.is_null() {
        return Ok(None);
    }
    if let Some(ts) = raw.as_i64() {
        return Ok(Some(UNIX_EPOCH + Duration::from_secs(ts.max(0) as u64)));
    }
    let s = raw.as_str().unwrap_or("").trim();
    if s.is_empty() {
        return Ok(None);
    }
    if let Ok(ts) = s.parse::<i64>() {
        return Ok(Some(UNIX_EPOCH + Duration::from_secs(ts.max(0) as u64)));
    }
    parse_iso8601(s).map(Some)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (m + 9).rem_euclid(12);
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn parse_iso8601(s: &str) -> Result<SystemTime> {
    let (dt, tz) = if let Some(stripped) = s.strip_suffix(['Z', 'z']) {
        (stripped, 0i64)
    } else if let Some(tpos) = s.rfind('T') {
        let rest = &s[tpos + 1..];
        match rest.rfind(['+', '-']) {
            Some(i) => {
                let (hh, mm) = rest[i + 1..]
                    .split_once(':')
                    .context("bad timezone offset")?;
                let secs: i64 = hh.parse::<i64>().context("bad tz hours")? * 3600
                    + mm.parse::<i64>().context("bad tz minutes")? * 60;
                (
                    &s[..tpos + 1 + i],
                    if rest[i..].starts_with('-') {
                        secs
                    } else {
                        -secs
                    },
                )
            }
            None => (s, 0),
        }
    } else {
        (s, 0)
    };
    let (date, time) = dt.split_once('T').context("expected YYYY-MM-DDTHH:MM:SS")?;
    let mut dp = date.split('-');
    let (y, m, d): (i64, i64, i64) = (
        dp.next().context("bad year")?.parse().context("bad year")?,
        dp.next()
            .context("bad month")?
            .parse()
            .context("bad month")?,
        dp.next().context("bad day")?.parse().context("bad day")?,
    );
    let mut tp = time.split(':');
    let (hh, mm, ss): (i64, i64, i64) = (
        tp.next().context("bad hour")?.parse().context("bad hour")?,
        tp.next()
            .context("bad minute")?
            .parse()
            .context("bad minute")?,
        tp.next()
            .unwrap_or("0")
            .split(['.', ','])
            .next()
            .unwrap()
            .parse()
            .context("bad second")?,
    );
    let epoch = days_from_civil(y, m, d) * 86400 + hh * 3600 + mm * 60 + ss + tz;
    Ok(UNIX_EPOCH + Duration::from_secs(epoch.max(0) as u64))
}

/// Download media bytes fully into memory, enforcing the size limit.
async fn download_bytes(client: &Client, media: &Media) -> Result<Vec<u8>> {
    if let Some(size) = media.size()
        && size > MAX_DOWNLOAD_BYTES
    {
        bail!("media is too large ({size} bytes, limit {MAX_DOWNLOAD_BYTES})");
    }
    let mut iter = client.iter_download(media);
    let mut buf = Vec::new();
    while let Some(chunk) = iter.next().await.context("download failed")? {
        if buf.len() + chunk.len() > MAX_DOWNLOAD_BYTES {
            bail!("media is too large (limit {MAX_DOWNLOAD_BYTES} bytes)");
        }
        buf.extend_from_slice(&chunk);
    }
    if buf.is_empty() {
        bail!("download produced no data");
    }
    Ok(buf)
}

/// Upload local files, checking they exist first.
async fn upload_many(client: &Client, paths: &[&str]) -> Result<Vec<Uploaded>> {
    let mut out = Vec::with_capacity(paths.len());
    for p in paths {
        if !std::path::Path::new(p).is_file() {
            bail!("file not found: {p}");
        }
        out.push(
            client
                .upload_file(p)
                .await
                .context(format!("upload failed: {p}"))?,
        );
    }
    Ok(out)
}

fn topic_id_arg(args: &Value) -> Option<i32> {
    args.get("topic_id")
        .and_then(|v| v.as_i64())
        .map(|v| v as i32)
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).context("failed to create output directory")?;
    }
    Ok(())
}

// ── tool handlers ────────────────────────────────────────────────────────

async fn send_file(client: &Client, args: &Value) -> Result<String> {
    let chat = &args["chat_id"];
    let label = chat_label(chat);
    let caption = str_arg(args, "caption");
    let topic_id = topic_id_arg(args);

    let paths: Vec<&str> = match &args["file_path"] {
        Value::String(s) => vec![s.as_str()],
        Value::Array(a) => a.iter().filter_map(|v| v.as_str()).collect(),
        _ => bail!("file_path must be a string or an array of strings"),
    };
    if paths.is_empty() || paths.len() > 10 {
        bail!("file_path must contain 1-10 files");
    }

    let peer = resolve_chat(client, chat).await?;
    let uploaded = upload_many(client, &paths).await?;

    if uploaded.len() == 1 {
        let schedule = parse_schedule_date(&args["schedule_date"])?;
        let mut msg = InputMessage::new();
        if !caption.is_empty() {
            msg = msg.text(caption);
        }
        msg = msg.reply_to(topic_id);
        if let Some(dt) = schedule {
            msg = msg.schedule_date(Some(dt));
        }
        let file = uploaded.into_iter().next().unwrap();
        msg = if is_photo_ext(paths[0]) {
            msg.photo(file)
        } else {
            msg.document(file)
        };
        client
            .send_message(peer, msg)
            .await
            .context("send failed")?;
        return Ok(match schedule {
            Some(_) => format!("file from {} scheduled in chat {label}.", paths[0]),
            None => format!("file sent to chat {label} from {}.", paths[0]),
        });
    }

    // 2-10 files → one media group.
    if parse_schedule_date(&args["schedule_date"])?.is_some() {
        bail!(
            "not supported: scheduling an album is not available in grammers 0.10 \
             (InputMedia has no schedule_date)"
        );
    }
    let mut medias = Vec::with_capacity(uploaded.len());
    for (i, file) in uploaded.into_iter().enumerate() {
        let mut m = InputMedia::new();
        if i == 0 {
            if !caption.is_empty() {
                m = m.caption(caption);
            }
            m = m.reply_to(topic_id);
        }
        m = if is_photo_ext(paths[i]) {
            m.photo(file)
        } else {
            m.document(file)
        };
        medias.push(m);
    }
    client
        .send_album(peer, medias)
        .await
        .context("send failed")?;
    Ok(format!(
        "album of {} files sent to chat {label}.",
        paths.len()
    ))
}

async fn send_album(client: &Client, args: &Value) -> Result<String> {
    let chat = &args["chat_id"];
    let label = chat_label(chat);
    let caption = str_arg(args, "caption");
    let topic_id = topic_id_arg(args);

    let paths: Vec<&str> = match &args["file_paths"] {
        Value::Array(a) => a.iter().filter_map(|v| v.as_str()).collect(),
        _ => bail!("file_paths must be an array of strings"),
    };
    if paths.len() < 2 || paths.len() > 10 {
        bail!("albums must contain between 2 and 10 files");
    }
    if parse_schedule_date(&args["schedule_date"])?.is_some() {
        bail!(
            "not supported: scheduling an album is not available in grammers 0.10 \
             (InputMedia has no schedule_date)"
        );
    }

    let peer = resolve_chat(client, chat).await?;
    let uploaded = upload_many(client, &paths).await?;
    let mut medias = Vec::with_capacity(uploaded.len());
    for (i, file) in uploaded.into_iter().enumerate() {
        let mut m = InputMedia::new();
        if i == 0 {
            if !caption.is_empty() {
                m = m.caption(caption);
            }
            m = m.reply_to(topic_id);
        }
        m = if is_photo_ext(paths[i]) {
            m.photo(file)
        } else {
            m.document(file)
        };
        medias.push(m);
    }
    client
        .send_album(peer, medias)
        .await
        .context("send failed")?;
    Ok(format!(
        "album of {} files sent to chat {label}.",
        paths.len()
    ))
}

async fn download_media(client: &Client, args: &Value) -> Result<String> {
    let chat = &args["chat_id"];
    let message_id = i64_arg(args, "message_id", 0) as i32;
    if message_id <= 0 {
        bail!("message_id is required");
    }
    let peer = resolve_chat(client, chat).await?;
    let msg = get_message(client, peer, message_id).await?;
    let media = msg
        .media()
        .with_context(|| format!("no media found in message {message_id}"))?;
    if let Some(size) = media.size()
        && size > MAX_DOWNLOAD_BYTES
    {
        bail!("media is too large for download_media (limit {MAX_DOWNLOAD_BYTES} bytes)");
    }

    let default_ext = match &media {
        Media::Document(d) => d.name().and_then(|n| n.rsplit('.').next()).unwrap_or("bin"),
        Media::Photo(_) => "jpg",
        _ => "bin",
    };
    let out_path = match args.get("file_path").and_then(|v| v.as_str()) {
        Some(p) if !p.trim().is_empty() => PathBuf::from(p),
        _ => temp_path(&format!("telegram_{message_id}"), default_ext),
    };
    ensure_parent(&out_path)?;
    client
        .download_media(&media, &out_path)
        .await
        .context("download failed")?;
    Ok(format!("media downloaded to {}.", out_path.display()))
}

async fn send_voice(client: &Client, args: &Value) -> Result<String> {
    let chat = &args["chat_id"];
    let label = chat_label(chat);
    let path = str_arg(args, "file_path");
    if path.is_empty() {
        bail!("file_path is required");
    }
    let lower = path.to_ascii_lowercase();
    if !(lower.ends_with(".ogg") || lower.ends_with(".opus")) {
        bail!("voice file must be .ogg or .opus format");
    }
    if !std::path::Path::new(path).is_file() {
        bail!("file not found: {path}");
    }
    let peer = resolve_chat(client, chat).await?;
    let uploaded = client.upload_file(path).await.context("upload failed")?;
    let msg = InputMessage::new()
        .document(uploaded)
        .attribute(Attribute::Voice {
            duration: Duration::ZERO,
            waveform: None,
        })
        .mime_type("audio/ogg")
        .reply_to(topic_id_arg(args));
    client
        .send_message(peer, msg)
        .await
        .context("send failed")?;
    Ok(format!("voice message sent to chat {label} from {path}."))
}

async fn upload_file(client: &Client, args: &Value) -> Result<String> {
    let path = str_arg(args, "file_path");
    if path.is_empty() {
        bail!("file_path is required");
    }
    let size = std::fs::metadata(path).context("file not found")?.len();
    let uploaded: Uploaded = client.upload_file(path).await.context("upload failed")?;
    let (name, md5) = match &uploaded.raw {
        tl::enums::InputFile::File(f) => (f.name.clone(), f.md5_checksum.clone()),
        _ => (String::new(), String::new()),
    };
    Ok(json!({
        "path": path,
        "name": name,
        "size": size,
        "md5_checksum": md5,
    })
    .to_string())
}

fn media_kind(media: &Media) -> &'static str {
    match media {
        Media::Photo(_) => "photo",
        Media::Document(_) => "document",
        Media::Sticker(_) => "sticker",
        Media::Contact(_) => "contact",
        Media::Poll(_) => "poll",
        Media::Geo(_) => "geo",
        Media::Dice(_) => "dice",
        Media::Venue(_) => "venue",
        Media::GeoLive(_) => "geo_live",
        Media::WebPage(_) => "webpage",
        _ => "other",
    }
}

async fn get_media_info(client: &Client, args: &Value) -> Result<String> {
    let chat = &args["chat_id"];
    let message_id = i64_arg(args, "message_id", 0) as i32;
    if message_id <= 0 {
        bail!("message_id is required");
    }
    let peer = resolve_chat(client, chat).await?;
    let msg = get_message(client, peer, message_id).await?;
    let media = msg
        .media()
        .with_context(|| format!("no media found in message {message_id}"))?;

    let mut info = json!({ "type": media_kind(&media) });
    match &media {
        Media::Photo(p) => {
            info["id"] = json!(p.id());
            info["size"] = json!(p.size());
            info["thumbs"] = json!(p.thumbs().len());
        }
        Media::Document(d) => {
            info["id"] = json!(d.id());
            info["name"] = json!(d.name());
            info["mime_type"] = json!(d.mime_type());
            info["size"] = json!(d.size());
            info["creation_date"] = json!(
                d.creation_date()
                    .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
            );
        }
        Media::Sticker(s) => {
            info["id"] = json!(s.document.id());
            info["name"] = json!(s.document.name());
            info["mime_type"] = json!(s.document.mime_type());
            info["size"] = json!(s.document.size());
        }
        _ => {}
    }
    Ok(serde_json::to_string_pretty(&info)?)
}

async fn get_sticker_sets(client: &Client, _args: &Value) -> Result<String> {
    let result = client
        .invoke(&tl::functions::messages::GetAllStickers { hash: 0 })
        .await
        .context("failed to get sticker sets")?;
    let titles: Vec<String> = match result {
        tl::enums::messages::AllStickers::Stickers(s) => s
            .sets
            .iter()
            .map(|set| match set {
                tl::enums::StickerSet::Set(st) => st.title.clone(),
            })
            .collect(),
        tl::enums::messages::AllStickers::NotModified => Vec::new(),
    };
    Ok(serde_json::to_string_pretty(&titles)?)
}

async fn send_sticker(client: &Client, args: &Value) -> Result<String> {
    let chat = &args["chat_id"];
    let label = chat_label(chat);
    let path = str_arg(args, "file_path");
    if path.is_empty() {
        bail!("file_path is required");
    }
    if !path.to_ascii_lowercase().ends_with(".webp") {
        bail!("sticker file should be a .webp file");
    }
    if !std::path::Path::new(path).is_file() {
        bail!("file not found: {path}");
    }
    let peer = resolve_chat(client, chat).await?;
    let uploaded = client.upload_file(path).await.context("upload failed")?;
    let msg = InputMessage::new()
        .document(uploaded)
        .mime_type("image/webp")
        .reply_to(topic_id_arg(args));
    client
        .send_message(peer, msg)
        .await
        .context("send failed")?;
    Ok(format!("sticker sent to chat {label} from {path}."))
}

async fn get_gif_search(client: &Client, args: &Value) -> Result<String> {
    let query = str_arg(args, "query");
    if query.is_empty() {
        bail!("query is required");
    }
    let limit = usize_arg(args, "limit", 10).clamp(1, 50);

    // NOTE: messages.searchGifs was removed from this TL layer, so GIF search
    // goes through an inline query to the official @gif bot instead.
    let bot = client
        .resolve_username("gif")
        .await
        .context("failed to resolve @gif")?
        .context("@gif bot not found")?;
    let bot_ref = bot.to_ref().await.context("failed to get bot peer")?;
    let result = client
        .invoke(&tl::functions::messages::GetInlineBotResults {
            bot: tl::enums::InputUser::from(&bot_ref),
            peer: tl::enums::InputPeer::from(&bot_ref),
            geo_point: None,
            query: query.to_string(),
            offset: String::new(),
        })
        .await
        .context("gif search failed")?;

    let ids: Vec<i64> = match result {
        tl::enums::messages::BotResults::Results(r) => r
            .results
            .iter()
            .filter_map(|res| match res {
                tl::enums::BotInlineResult::BotInlineMediaResult(m) => {
                    m.document.as_ref().and_then(|d| match d {
                        tl::enums::Document::Document(doc) => Some(doc.id),
                        _ => None,
                    })
                }
                _ => None,
            })
            .take(limit)
            .collect(),
    };
    Ok(serde_json::to_string_pretty(&ids)?)
}

async fn send_gif(_client: &Client, args: &Value) -> Result<String> {
    let gif_id = i64_arg(args, "gif_id", 0);
    // TODO: implement once grammers can turn a bare document id into an
    // InputDocument. Sending needs (id, access_hash, file_reference); the
    // search result holds all three, but grammers 0.10 has no id → document
    // resolution, and this tool only receives the id.
    bail!(
        "not supported: sending a GIF by document id ({gif_id}) requires the \
         document's access_hash, which grammers 0.10 cannot resolve from an id alone"
    )
}

async fn list_photos(client: &Client, args: &Value) -> Result<String> {
    let chat = &args["chat_id"];
    let label = chat_label(chat);
    let source = str_arg(args, "source");
    let source = if source.is_empty() { "avatars" } else { source };
    if source != "avatars" && source != "messages" {
        bail!("unknown photo source; expected one of: avatars, messages");
    }
    let limit = usize_arg(args, "limit", 20).clamp(1, 100);
    let peer = resolve_chat(client, chat).await?;

    let mut photos = Vec::new();
    if source == "avatars" {
        // Profile photos are only implemented for users.
        if !matches!(peer.id.kind(), PeerKind::User | PeerKind::UserSelf) {
            bail!("not supported: avatar listing is only implemented for users");
        }
        let result = client
            .invoke(&tl::functions::photos::GetUserPhotos {
                user_id: tl::enums::InputUser::from(&peer),
                offset: 0,
                max_id: 0,
                limit: limit as i32,
            })
            .await
            .context("failed to get profile photos")?;
        for photo in result.photos().into_iter().take(limit) {
            let (id, date) = match &photo {
                tl::enums::Photo::Photo(p) => (p.id, p.date),
                tl::enums::Photo::Empty(p) => (p.id, 0),
            };
            photos.push(json!({ "id": id, "date": date }));
        }
    } else {
        let mut iter = client.iter_messages(peer);
        while let Some(msg) = iter.next().await.context("failed to fetch messages")? {
            if matches!(msg.media(), Some(Media::Photo(_))) {
                photos.push(json!({
                    "id": msg.id(),
                    "date": msg.date().format("%Y-%m-%d %H:%M:%S").to_string(),
                }));
                if photos.len() >= limit {
                    break;
                }
            }
        }
    }

    Ok(serde_json::to_string_pretty(&json!({
        "chat_id": label,
        "source": source,
        "count": photos.len(),
        "photos": photos,
    }))?)
}

/// Download one photo (avatar or message photo) and return the temp file path.
/// Path text is returned because this crate's CallToolResult has no image content.
async fn open_photo(client: &Client, args: &Value) -> Result<String> {
    let chat = &args["chat_id"];
    let label = chat_label(chat);
    let peer = resolve_chat(client, chat).await?;

    let photo = if let Some(message_id) = args.get("message_id").and_then(|v| v.as_i64()) {
        let msg = get_message(client, peer, message_id as i32).await?;
        match msg.media() {
            Some(Media::Photo(p)) => p,
            _ => bail!("message {message_id} has no photo"),
        }
    } else {
        if !matches!(peer.id.kind(), PeerKind::User | PeerKind::UserSelf) {
            bail!("not supported: avatar opening is only implemented for users");
        }
        let wanted = args.get("photo_id").and_then(|v| v.as_i64());
        let result = client
            .invoke(&tl::functions::photos::GetUserPhotos {
                user_id: tl::enums::InputUser::from(&peer),
                offset: 0,
                max_id: 0,
                limit: 100,
            })
            .await
            .context("failed to get profile photos")?;
        let found = result.photos().into_iter().find(|p| match (wanted, p) {
            (Some(w), tl::enums::Photo::Photo(ph)) => ph.id == w,
            (Some(w), tl::enums::Photo::Empty(ph)) => ph.id == w,
            (None, _) => true,
        });
        let found = found.with_context(|| {
            wanted
                .map(|w| format!("no avatar photo with id {w} for chat {label}"))
                .unwrap_or_else(|| format!("no avatar photos for chat {label}"))
        })?;
        grammers_client::media::Photo::from_raw(found)
    };

    let out_path = match args.get("save_path").and_then(|v| v.as_str()) {
        Some(p) if !p.trim().is_empty() => PathBuf::from(p),
        _ => temp_path("telegram_photo", "jpg"),
    };
    ensure_parent(&out_path)?;
    client
        .download_media(&photo, &out_path)
        .await
        .context("photo download failed")?;
    Ok(format!(
        "photo saved to {}. (This server returns the file path instead of MCP image content.)",
        out_path.display()
    ))
}

/// Download up to 12 thumbnails and return their paths.
/// TODO: compose a labelled contact sheet — needs an image crate (e.g. `image`).
async fn get_photo_sheet(client: &Client, args: &Value) -> Result<String> {
    let chat = &args["chat_id"];
    let source = str_arg(args, "source");
    let source = if source.is_empty() { "avatars" } else { source };
    if source != "avatars" && source != "messages" {
        bail!("unknown photo source; expected one of: avatars, messages");
    }
    let limit = usize_arg(args, "limit", 6).clamp(1, 12);
    let peer = resolve_chat(client, chat).await?;

    let mut items: Vec<(i64, grammers_client::media::Photo)> = Vec::new();
    if source == "avatars" {
        if !matches!(peer.id.kind(), PeerKind::User | PeerKind::UserSelf) {
            bail!("not supported: avatar listing is only implemented for users");
        }
        let result = client
            .invoke(&tl::functions::photos::GetUserPhotos {
                user_id: tl::enums::InputUser::from(&peer),
                offset: 0,
                max_id: 0,
                limit: limit as i32,
            })
            .await
            .context("failed to get profile photos")?;
        for p in result.photos().into_iter().take(limit) {
            let id = match &p {
                tl::enums::Photo::Photo(ph) => ph.id,
                tl::enums::Photo::Empty(ph) => ph.id,
            };
            items.push((id, grammers_client::media::Photo::from_raw(p)));
        }
    } else {
        let mut iter = client.iter_messages(peer);
        while let Some(msg) = iter.next().await.context("failed to fetch messages")? {
            if let Some(Media::Photo(p)) = msg.media() {
                items.push((msg.id() as i64, p));
                if items.len() >= limit {
                    break;
                }
            }
        }
    }
    if items.is_empty() {
        bail!("no {source} photos found for chat {}", chat_label(chat));
    }

    let mut lines = Vec::new();
    for (id, photo) in &items {
        let mut thumbs = photo.thumbs();
        thumbs.sort_by_key(|t| t.size());
        let thumb = thumbs
            .into_iter()
            .next()
            .with_context(|| format!("photo {id} has no downloadable size"))?;
        let path = temp_path(&format!("telegram_thumb_{id}"), "jpg");
        client
            .download_media(&thumb, &path)
            .await
            .with_context(|| format!("thumbnail download failed for photo {id}"))?;
        lines.push(format!("- id {id} -> {}", path.display()));
    }
    Ok(format!(
        "{} {source} photo(s). Thumbnails saved as individual files \
         (sheet composition needs an image crate):\n{}",
        lines.len(),
        lines.join("\n")
    ))
}

async fn inspect_document(client: &Client, args: &Value) -> Result<String> {
    let chat = &args["chat_id"];
    let message_id = i64_arg(args, "message_id", 0) as i32;
    if message_id <= 0 {
        bail!("message_id is required");
    }
    let peer = resolve_chat(client, chat).await?;
    let msg = get_message(client, peer, message_id).await?;
    let media = msg
        .media()
        .with_context(|| format!("no document or media in message {message_id}"))?;

    // Download strictly into memory (no disk footprint for the read itself).
    let data = download_bytes(client, &media).await?;

    let (filename, mime) = match &media {
        Media::Document(d) => (
            d.name().unwrap_or("").to_string(),
            d.mime_type().unwrap_or("").to_string(),
        ),
        Media::Photo(_) => ("photo.jpg".to_string(), "image/jpeg".to_string()),
        Media::Sticker(_) => ("sticker.webp".to_string(), "image/webp".to_string()),
        _ => (String::new(), String::new()),
    };
    let ext = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let is_image = mime.starts_with("image/")
        || matches!(
            ext.as_str(),
            "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp"
        );
    let is_text = mime.starts_with("text/")
        || matches!(
            ext.as_str(),
            "txt" | "csv" | "json" | "md" | "log" | "yaml" | "yml" | "xml" | "html"
        );

    if is_image {
        // No image content support in CallToolResult → save and return the path.
        let path = temp_path("telegram_inspect", &ext);
        std::fs::write(&path, &data).context("failed to write temp file")?;
        return Ok(format!(
            "image saved to {}. (This server returns the file path instead of MCP image content.)",
            path.display()
        ));
    }
    if mime == "application/pdf" || ext == "pdf" {
        // TODO: PDF text extraction needs a PDF parsing crate (e.g. `pdf-extract`).
        let path = temp_path("telegram_inspect", "pdf");
        std::fs::write(&path, &data).context("failed to write temp file")?;
        return Ok(format!(
            "PDF '{filename}' ({} bytes) saved to {}. PDF text extraction is not \
             implemented in this port — it needs a PDF parsing crate.",
            data.len(),
            path.display()
        ));
    }
    if is_text {
        let text = String::from_utf8_lossy(&data);
        let mut chars = text.chars();
        let head: String = chars.by_ref().take(MAX_TEXT_CHARS).collect();
        return Ok(if chars.next().is_some() {
            format!("{head}\n… [truncated at {MAX_TEXT_CHARS} chars]")
        } else {
            head
        });
    }

    let path = temp_path(
        "telegram_inspect",
        if ext.is_empty() { "bin" } else { &ext },
    );
    std::fs::write(&path, &data).context("failed to write temp file")?;
    Ok(format!(
        "file format '{filename}' ({mime}) is not supported for direct text analysis; \
         saved to {}.",
        path.display()
    ))
}

// ── definitions & dispatch ───────────────────────────────────────────────

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "send_file".into(),
            description: "Send a file to a chat. Pass an array of 2-10 paths to send them as one media group. Supports caption, forum topic reply, and scheduling (single file only).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": { "type": ["integer", "string"], "description": "Chat ID or username" },
                    "file_path": { "type": ["string", "array"], "description": "Local file path, or 2-10 paths for a media group", "items": { "type": "string" } },
                    "caption": { "type": "string", "description": "Optional caption" },
                    "topic_id": { "type": "integer", "description": "Optional forum topic ID (also acts as reply_to)" },
                    "schedule_date": { "type": ["integer", "string"], "description": "Optional unix timestamp or ISO-8601 datetime (UTC). Single file only." }
                },
                "required": ["chat_id", "file_path"]
            }),
        },
        ToolDefinition {
            name: "send_album".into(),
            description: "Send 2-10 photos/videos as one Telegram media group (album). Telegram shows the caption on the first item.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": { "type": ["integer", "string"], "description": "Chat ID or username" },
                    "file_paths": { "type": "array", "description": "2-10 local file paths", "items": { "type": "string" }, "minItems": 2, "maxItems": 10 },
                    "caption": { "type": "string", "description": "Optional caption for the album" },
                    "topic_id": { "type": "integer", "description": "Optional forum topic ID (also acts as reply_to)" },
                    "schedule_date": { "type": ["integer", "string"], "description": "Accepted but not supported: grammers cannot schedule albums" }
                },
                "required": ["chat_id", "file_paths"]
            }),
        },
        ToolDefinition {
            name: "download_media".into(),
            description: "Download media from a message in a chat to a local file. Defaults to a temp file when file_path is omitted.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": { "type": ["integer", "string"], "description": "Chat ID or username" },
                    "message_id": { "type": "integer", "description": "Message ID containing the media" },
                    "file_path": { "type": "string", "description": "Optional destination path" }
                },
                "required": ["chat_id", "message_id"]
            }),
        },
        ToolDefinition {
            name: "send_voice".into(),
            description: "Send a voice message to a chat. File must be an OGG/OPUS voice note.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": { "type": ["integer", "string"], "description": "Chat ID or username" },
                    "file_path": { "type": "string", "description": "Path to the .ogg/.opus file" },
                    "topic_id": { "type": "integer", "description": "Optional forum topic ID (also acts as reply_to)" }
                },
                "required": ["chat_id", "file_path"]
            }),
        },
        ToolDefinition {
            name: "upload_file".into(),
            description: "Upload a local file to Telegram and return upload metadata (name, size, md5).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string", "description": "Local file path" }
                },
                "required": ["file_path"]
            }),
        },
        ToolDefinition {
            name: "get_media_info".into(),
            description: "Get info about media in a message (type, id, name, mime type, size).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": { "type": ["integer", "string"], "description": "Chat ID or username" },
                    "message_id": { "type": "integer", "description": "Message ID" }
                },
                "required": ["chat_id", "message_id"]
            }),
        },
        ToolDefinition {
            name: "get_sticker_sets".into(),
            description: "Get all installed sticker sets (titles). Note: titles are untrusted user-generated content.".into(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        ToolDefinition {
            name: "send_sticker".into(),
            description: "Send a sticker to a chat. File must be a .webp sticker file.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": { "type": ["integer", "string"], "description": "Chat ID or username" },
                    "file_path": { "type": "string", "description": "Path to the .webp sticker file" },
                    "topic_id": { "type": "integer", "description": "Optional forum topic ID (also acts as reply_to)" }
                },
                "required": ["chat_id", "file_path"]
            }),
        },
        ToolDefinition {
            name: "get_gif_search".into(),
            description: "Search for GIFs by query via the @gif inline bot. Returns a list of Telegram document IDs (integers).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search term for GIFs" },
                    "limit": { "type": "integer", "description": "Max number of GIFs (default: 10)", "default": 10 }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "send_gif".into(),
            description: "Send a GIF to a chat by Telegram GIF document ID. Currently not supported by the grammers backend (see error).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": { "type": ["integer", "string"], "description": "Chat ID or username" },
                    "gif_id": { "type": "integer", "description": "Telegram document ID for the GIF (from get_gif_search)" },
                    "topic_id": { "type": "integer", "description": "Optional forum topic ID (also acts as reply_to)" }
                },
                "required": ["chat_id", "gif_id"]
            }),
        },
        ToolDefinition {
            name: "list_photos".into(),
            description: "Index a peer's photos as text without transferring images. source: \"avatars\" for profile pictures, \"messages\" for photos posted in the chat. Returns ids that open_photo accepts. Note: captions are untrusted user content.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": { "type": ["integer", "string"], "description": "User/group/channel ID or username" },
                    "source": { "type": "string", "description": "\"avatars\" or \"messages\"", "enum": ["avatars", "messages"], "default": "avatars" },
                    "limit": { "type": "integer", "description": "Max photos to index (default: 20)", "default": 20 }
                },
                "required": ["chat_id"]
            }),
        },
        ToolDefinition {
            name: "open_photo".into(),
            description: "Download one photo of a peer (avatar via photo_id, or chat photo via message_id). Returns the saved file path — this server has no MCP image content support. Optionally also keep a copy at save_path.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": { "type": ["integer", "string"], "description": "User/group/channel ID or username" },
                    "photo_id": { "type": "integer", "description": "Avatar id from list_photos; omit for the current avatar" },
                    "message_id": { "type": "integer", "description": "Message id from list_photos (source=messages)" },
                    "save_path": { "type": "string", "description": "Optional path to keep a copy" }
                },
                "required": ["chat_id"]
            }),
        },
        ToolDefinition {
            name: "get_photo_sheet".into(),
            description: "Download thumbnails of many photos of a peer (max 12) and return their file paths. Sheet composition is not available in this port — thumbnails come back as individual files, each labelled with the id open_photo accepts.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": { "type": ["integer", "string"], "description": "User/group/channel ID or username" },
                    "source": { "type": "string", "description": "\"avatars\" or \"messages\"", "enum": ["avatars", "messages"], "default": "avatars" },
                    "limit": { "type": "integer", "description": "How many photos (default: 6, max: 12)", "default": 6 },
                    "columns": { "type": "integer", "description": "Accepted but ignored: no sheet is composed" }
                },
                "required": ["chat_id"]
            }),
        },
        ToolDefinition {
            name: "inspect_document".into(),
            description: "Inspect a document/image from a Telegram message, downloaded in memory. Returns extracted text for text files, or a saved file path for images, PDFs (no PDF text extraction in this port) and other formats.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": { "type": ["integer", "string"], "description": "Chat ID or username" },
                    "message_id": { "type": "integer", "description": "Message ID with the document/media" }
                },
                "required": ["chat_id", "message_id"]
            }),
        },
    ]
}

pub async fn try_handle(
    client: &Client,
    tool_name: &str,
    args: &Value,
) -> anyhow::Result<Option<CallToolResult>> {
    let text = match tool_name {
        "send_file" => send_file(client, args).await?,
        "send_album" => send_album(client, args).await?,
        "download_media" => download_media(client, args).await?,
        "send_voice" => send_voice(client, args).await?,
        "upload_file" => upload_file(client, args).await?,
        "get_media_info" => get_media_info(client, args).await?,
        "send_sticker" => send_sticker(client, args).await?,
        "get_sticker_sets" => get_sticker_sets(client, args).await?,
        "get_gif_search" => get_gif_search(client, args).await?,
        "send_gif" => send_gif(client, args).await?,
        "list_photos" => list_photos(client, args).await?,
        "open_photo" => open_photo(client, args).await?,
        "get_photo_sheet" => get_photo_sheet(client, args).await?,
        "inspect_document" => inspect_document(client, args).await?,
        _ => return Ok(None),
    };
    Ok(Some(CallToolResult::ok(text)))
}
