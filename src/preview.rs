//! Local HTTP server for the HTML preview.
//!
//! Serves the generated `index.html` plus the `attachments/` directory out of the
//! output folder and opens the browser. Runs until interrupted (Ctrl-C).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use anyhow::{Context, Result};
use tiny_http::{Header, Request, Response, Server};

fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "html" | "htm" => "text/html; charset=utf-8",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        // Video, so the preview's <video> element can actually play attachments.
        "mp4" => "video/mp4",
        "m4v" => "video/x-m4v",
        "mov" | "qt" => "video/quicktime",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        "3gp" => "video/3gpp",
        "avi" => "video/x-msvideo",
        "mkv" => "video/x-matroska",
        // Audio.
        "m4a" => "audio/mp4",
        "mp3" => "audio/mpeg",
        "aac" => "audio/aac",
        "wav" => "audio/wav",
        "ogg" | "oga" => "audio/ogg",
        "caf" => "audio/x-caf",
        _ => "application/octet-stream",
    }
}

/// Serve `out_dir` at `http://127.0.0.1:port/` and open a browser.
///
/// Requests are handled by a small pool of worker threads sharing the `Server`, so a
/// page loading many images doesn't stall behind a single serial request loop.
pub fn serve(out_dir: &Path, port: u16) -> Result<()> {
    let addr = format!("127.0.0.1:{port}");
    let server = Arc::new(
        Server::http(&addr)
            .map_err(|e| anyhow::anyhow!("starting preview server on {addr}: {e}"))?,
    );
    let url = format!("http://{addr}/");
    println!("Preview ready at {url}\nPress Ctrl-C to stop.");
    if let Err(e) = open::that(&url) {
        eprintln!("(could not open browser automatically: {e})");
    }

    let out_dir = out_dir.to_path_buf();
    let workers = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(2, 8);
    let mut handles = Vec::with_capacity(workers);
    for _ in 0..workers {
        let server = Arc::clone(&server);
        let out_dir = out_dir.clone();
        handles.push(thread::spawn(move || {
            for request in server.incoming_requests() {
                handle_request(request, &out_dir);
            }
        }));
    }
    for h in handles {
        let _ = h.join();
    }
    Ok(())
}

/// The `Range: bytes=…` header of a request, if present.
fn range_spec(request: &Request) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case("range"))
        .map(|h| h.value.as_str().to_string())
}

/// Parse a single `bytes=start-end` range against a resource of `len` bytes, returning
/// an inclusive `(start, end)`. Handles open-ended (`start-`) and suffix (`-n`) forms and
/// rejects anything unsatisfiable.
fn parse_range(spec: &str, len: u64) -> Option<(u64, u64)> {
    // Guard up front so the `len - 1` arithmetic below can never underflow on an empty
    // resource (a 0-byte file). Without this, a `bytes=0-` / suffix request would panic
    // in debug builds and take down the worker thread.
    if len == 0 {
        return None;
    }
    let spec = spec.trim().strip_prefix("bytes=")?;
    let (a, b) = spec.split_once('-')?;
    let (start, end) = if a.is_empty() {
        // Suffix range: the last `b` bytes.
        let n: u64 = b.parse().ok()?;
        if n == 0 {
            return None;
        }
        (len.saturating_sub(n), len - 1)
    } else {
        let start: u64 = a.parse().ok()?;
        let end: u64 = if b.is_empty() {
            len - 1
        } else {
            b.parse().ok()?
        };
        (start, end.min(len - 1))
    };
    if start > end || start >= len {
        return None;
    }
    Some((start, end))
}

/// Resolve one request against the output directory and respond.
fn handle_request(request: Request, out_dir: &Path) {
    let raw = request.url().split('?').next().unwrap_or("/");
    let rel = raw.trim_start_matches('/');
    let rel = if rel.is_empty() { "index.html" } else { rel };
    // Prevent path traversal by dropping any `..` components.
    let safe: PathBuf = rel
        .split('/')
        .filter(|c| !c.is_empty() && *c != "..")
        .collect();
    let path = out_dir.join(&safe);

    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(_) => {
            let _ = request.respond(Response::from_string("Not found").with_status_code(404));
            return;
        }
    };

    let ctype =
        Header::from_bytes(&b"Content-Type"[..], content_type(&path).as_bytes()).expect("header");
    // Advertise range support so browsers (Safari especially) will stream <video>/<audio>.
    let accept = Header::from_bytes(&b"Accept-Ranges"[..], &b"bytes"[..]).expect("header");
    let len = bytes.len() as u64;

    // Honor a byte-range request with a 206 partial response so media can seek/stream.
    let response =
        if let Some((start, end)) = range_spec(&request).and_then(|s| parse_range(&s, len)) {
            let slice = bytes[start as usize..=end as usize].to_vec();
            let cr = Header::from_bytes(
                &b"Content-Range"[..],
                format!("bytes {start}-{end}/{len}").as_bytes(),
            )
            .expect("header");
            request.respond(
                Response::from_data(slice)
                    .with_status_code(206)
                    .with_header(ctype)
                    .with_header(accept)
                    .with_header(cr),
            )
        } else {
            request.respond(
                Response::from_data(bytes)
                    .with_header(ctype)
                    .with_header(accept),
            )
        };
    if let Err(e) = response {
        eprintln!("(preview response error: {e})");
    }
}

/// Write the rendered HTML to `out_dir/index.html`.
pub fn write_index(out_dir: &Path, html: &str) -> Result<()> {
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("creating output dir {}", out_dir.display()))?;
    let index = out_dir.join("index.html");
    std::fs::write(&index, html).with_context(|| format!("writing {}", index.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_type_covers_media() {
        assert_eq!(content_type(Path::new("clip.mp4")), "video/mp4");
        assert_eq!(content_type(Path::new("clip.MOV")), "video/quicktime");
        assert_eq!(content_type(Path::new("loop.gif")), "image/gif");
        assert_eq!(content_type(Path::new("voice.m4a")), "audio/mp4");
        assert_eq!(
            content_type(Path::new("mystery.xyz")),
            "application/octet-stream"
        );
    }

    #[test]
    fn range_parsing_forms_and_bounds() {
        assert_eq!(parse_range("bytes=0-99", 1000), Some((0, 99)));
        assert_eq!(parse_range("bytes=100-", 1000), Some((100, 999)));
        assert_eq!(parse_range("bytes=-100", 1000), Some((900, 999)));
        // End past EOF is clamped.
        assert_eq!(parse_range("bytes=990-5000", 1000), Some((990, 999)));
        // Malformed / unsatisfiable inputs are rejected.
        assert_eq!(parse_range("bytes=2000-3000", 1000), None);
        assert_eq!(parse_range("bytes=500-100", 1000), None);
        assert_eq!(parse_range("bytes=-0", 1000), None);
        assert_eq!(parse_range("kb=0-1", 1000), None);
        // A zero-length resource must never underflow, regardless of the range form.
        assert_eq!(parse_range("bytes=0-0", 0), None);
        assert_eq!(parse_range("bytes=0-", 0), None);
        assert_eq!(parse_range("bytes=-100", 0), None);
        assert_eq!(parse_range("bytes=5-9", 0), None);
    }
}
