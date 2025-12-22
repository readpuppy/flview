use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use axum::{
    extract::{Multipart, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Router,
};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use tokio::fs;
use tokio_util::io::ReaderStream;
use tracing::{info, warn};

use crate::fsutil;

#[derive(Clone)]
struct AppState {
    root: PathBuf,
}

#[derive(Debug, serde::Deserialize)]
struct BrowseQuery {
    path: Option<String>,
    mode: Option<String>,
}

pub async fn run(root: PathBuf, addr: String) -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let root = fsutil::canonicalize_root(&root)?;
    let state = AppState { root };

    let app = Router::new()
        .route("/", get(index))
        .route("/view", get(view_file))
        .route("/download", get(download))
        .route("/upload", post(upload))
        .with_state(state);

    let addr: SocketAddr = addr.parse().context("Invalid --addr")?;
    info!("Web listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn index(
    State(st): State<AppState>,
    Query(q): Query<BrowseQuery>,
) -> Result<Html<String>, Response> {
    let rel = q.path.clone().unwrap_or_default();
    let full = fsutil::resolve_under_root(&st.root, &rel).map_err(bad_request)?;

    let meta = fs::metadata(&full).await.map_err(not_found)?;
    if meta.is_file() {
        return Ok(Html(render_file_page(&full, "text", &rel, None).await));
    }

    let entries = fsutil::list_dir(&full).map_err(internal_error)?;
    Ok(Html(render_dir_page(&st.root, &full, &rel, &entries)))
}

async fn view_file(
    State(st): State<AppState>,
    Query(q): Query<BrowseQuery>,
) -> Result<Html<String>, Response> {
    let rel = q.path.clone().unwrap_or_default();
    let mode = q.mode.clone().unwrap_or_else(|| "text".to_string());
    let full = fsutil::resolve_under_root(&st.root, &rel).map_err(bad_request)?;

    let meta = fs::metadata(&full).await.map_err(not_found)?;
    if !meta.is_file() {
        return Err(bad_request("Not a file"));
    }

    let max = 512 * 1024;
    let bytes = tokio::task::spawn_blocking({
        let full = full.clone();
        move || crate::fsutil::read_file_limited(&full, max)
    })
    .await
    .map_err(internal_error)?
    .map_err(internal_error)?;

    Ok(Html(
        render_file_page(&full, &mode, &rel, Some(bytes)).await,
    ))
}

async fn download(
    State(st): State<AppState>,
    Query(q): Query<BrowseQuery>,
) -> Result<Response, Response> {
    let rel = q.path.clone().unwrap_or_default();
    let full = fsutil::resolve_under_root(&st.root, &rel).map_err(bad_request)?;

    let meta = fs::metadata(&full).await.map_err(not_found)?;
    if !meta.is_file() {
        return Err(bad_request("Not a file"));
    }

    let file = fs::File::open(&full).await.map_err(not_found)?;
    let stream = ReaderStream::new(file);
    let body = axum::body::Body::from_stream(stream);

    let name = full
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "download".to_string());

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{}\"", name)
            .parse()
            .map_err(internal_error)?,
    );

    Ok((headers, body).into_response())
}

async fn upload(
    State(st): State<AppState>,
    Query(q): Query<BrowseQuery>,
    mut multipart: Multipart,
) -> Result<Response, Response> {
    let rel = q.path.clone().unwrap_or_default();
    let dir = fsutil::resolve_under_root(&st.root, &rel).map_err(bad_request)?;

    let meta = fs::metadata(&dir).await.map_err(not_found)?;
    if !meta.is_dir() {
        return Err(bad_request("Upload target is not a directory"));
    }

    while let Some(field) = multipart.next_field().await.map_err(bad_request)? {
        let file_name = match field.file_name() {
            Some(n) => n.to_string(),
            None => continue,
        };

        // only keep filename component
        let safe_name = Path::new(&file_name)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "upload.bin".to_string());

        let dest = dir.join(&safe_name);
        let data = field.bytes().await.map_err(bad_request)?;

        if let Err(e) = fs::write(&dest, &data).await {
            warn!("upload write failed: {}", e);
            return Err(internal_error(e));
        }

        info!("uploaded {} ({} bytes)", dest.display(), data.len());
    }

    Ok(Redirect::to(&format!("/?path={}", urlenc(&rel))).into_response())
}

fn render_dir_page(root: &Path, dir: &Path, rel: &str, entries: &[fsutil::EntryInfo]) -> String {
    let title = format!("Index of /{}", rel);
    let up = parent_rel(rel);

    let mut rows = String::new();
    if dir != root {
        rows.push_str(&format!(
            "<li><a href=\"/?path={}\">[..]</a></li>",
            urlenc(&up)
        ));
    }

    for e in entries {
        let name = e.name.to_string_lossy();
        let child_rel = if rel.is_empty() {
            name.to_string()
        } else {
            format!("{}/{}", rel.trim_end_matches('/'), name)
        };

        if e.is_dir {
            rows.push_str(&format!(
                "<li>[DIR] <a href=\"/?path={}\">{}/</a></li>",
                urlenc(&child_rel),
                html_escape(&name)
            ));
        } else {
            rows.push_str(&format!(
                "<li>[FILE] <a href=\"/view?path={}&mode=text\">{}</a> ( <a href=\"/download?path={}\">download</a> )</li>",
                urlenc(&child_rel),
                html_escape(&name),
                urlenc(&child_rel)
            ));
        }
    }

    let back_to = if dir == root {
        "".to_string()
    } else {
        format!("<p><a href=\"/?path={}\">Up</a></p>", urlenc(&up))
    };

    format!(
        r#"<!doctype html>
<html>
<head><meta charset="utf-8" /><title>{}</title></head>
<body>
<h1>{}</h1>
{}
<form action="/upload?path={}" method="post" enctype="multipart/form-data">
    <input type="file" name="file" />
    <button type="submit">Upload</button>
</form>
<ul>
{}
</ul>
</body>
</html>"#,
        html_escape(&title),
        html_escape(&title),
        back_to,
        urlenc(rel),
        rows
    )
}

async fn render_file_page(full: &Path, mode: &str, rel: &str, bytes: Option<Vec<u8>>) -> String {
    let file_name = full
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| rel.to_string());

    let bytes = bytes.unwrap_or_default();
    let content = match mode {
        "hex" => {
            let lines = crate::hex::format_hex_lines(&bytes, 0, 4096);
            html_escape(&lines.join("\n"))
        }
        _ => html_escape(&String::from_utf8_lossy(&bytes)),
    };

    let switch_to = if mode == "hex" { "text" } else { "hex" };

    format!(
        r#"<!doctype html>
<html>
<head><meta charset="utf-8" /><title>{}</title></head>
<body>
<h1>{}</h1>
<p>
    <a href="/?path={}">Back</a> |
    <a href="/download?path={}">Download</a> |
    <a href="/view?path={}&mode={}">Switch to {}</a>
</p>
<pre style="white-space: pre; overflow: auto;">{}</pre>
</body>
</html>"#,
        html_escape(&file_name),
        html_escape(&file_name),
        urlenc(&parent_rel(rel)),
        urlenc(rel),
        urlenc(rel),
        switch_to,
        html_escape(switch_to),
        content
    )
}

fn parent_rel(rel: &str) -> String {
    let rel = rel.trim_matches('/');
    if rel.is_empty() {
        return "".to_string();
    }
    match rel.rfind('/') {
        Some(idx) => rel[..idx].to_string(),
        None => "".to_string(),
    }
}

fn urlenc(s: &str) -> String {
    utf8_percent_encode(s, NON_ALPHANUMERIC).to_string()
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn bad_request<E: std::fmt::Display>(e: E) -> Response {
    (StatusCode::BAD_REQUEST, e.to_string()).into_response()
}

fn not_found<E: std::fmt::Display>(e: E) -> Response {
    (StatusCode::NOT_FOUND, e.to_string()).into_response()
}

fn internal_error<E: std::fmt::Display>(e: E) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
}
