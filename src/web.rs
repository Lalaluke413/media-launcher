use eframe::egui;
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::mpsc::{self, Receiver, Sender},
    time::Duration,
};

pub struct Request {
    pub url: Box<str>,
    pub reply: Sender<Result<(), String>>,
}

pub struct Server {
    requests: Receiver<Request>,
}

impl Server {
    pub fn new(address: SocketAddr, ctx: egui::Context) -> Self {
        let (send, requests) = mpsc::channel();
        std::thread::spawn(move || serve(address, send, ctx));
        Self { requests }
    }

    pub fn try_recv(&self) -> Option<Request> {
        self.requests.try_recv().ok()
    }
}

fn serve(address: SocketAddr, requests: Sender<Request>, ctx: egui::Context) {
    let listener = match TcpListener::bind(address) {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("Could not start web server on {address}: {e}");
            return;
        }
    };
    eprintln!("Web player listening on http://{address}");

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => handle(&mut stream, &requests, &ctx),
            Err(e) => eprintln!("Web server connection failed: {e}"),
        }
    }
}

fn handle(stream: &mut TcpStream, requests: &Sender<Request>, ctx: &egui::Context) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut buffer = [0_u8; 8192];
    let size = match stream.read(&mut buffer) {
        Ok(size) => size,
        Err(_) => return,
    };
    let request = String::from_utf8_lossy(&buffer[..size]);
    let Some(line) = request.lines().next() else {
        return;
    };

    if line == "GET / HTTP/1.1" || line == "GET / HTTP/1.0" {
        respond(stream, "200 OK", "text/html; charset=utf-8", PAGE);
        return;
    }

    if line == "POST /play HTTP/1.1" || line == "POST /play HTTP/1.0" {
        let Some((_, body)) = request.split_once("\r\n\r\n") else {
            respond(stream, "400 Bad Request", "text/plain", "Bad request\n");
            return;
        };
        let Some(url) = form_url(body) else {
            respond(
                stream,
                "400 Bad Request",
                "text/plain",
                "Enter an http:// or https:// URL\n",
            );
            return;
        };
        let (reply, result) = mpsc::channel();
        if requests
            .send(Request {
                url: url.into(),
                reply,
            })
            .is_err()
        {
            respond(
                stream,
                "503 Service Unavailable",
                "text/plain",
                "Launcher unavailable\n",
            );
            return;
        }
        ctx.request_repaint();
        match result.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(())) => redirect(stream),
            Ok(Err(_)) => respond(
                stream,
                "409 Conflict",
                "text/plain",
                "Playback is already active\n",
            ),
            Err(_) => respond(
                stream,
                "503 Service Unavailable",
                "text/plain",
                "Launcher unavailable\n",
            ),
        }
        return;
    }

    respond(stream, "404 Not Found", "text/plain", "Not found\n");
}

fn form_url(body: &str) -> Option<String> {
    let encoded = body
        .split('&')
        .find_map(|field| field.strip_prefix("url="))?;
    let decoded = percent_decode(encoded)?;
    (decoded.starts_with("http://") || decoded.starts_with("https://")).then_some(decoded)
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 2;
            }
            b'%' => return None,
            byte => out.push(byte),
        }
        i += 1;
    }
    String::from_utf8(out).ok()
}

fn respond(stream: &mut TcpStream, status: &str, content_type: &str, body: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

fn redirect(stream: &mut TcpStream) {
    let _ = write!(
        stream,
        "HTTP/1.1 303 See Other\r\nLocation: /\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
}

const PAGE: &str = r#"<!doctype html>
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Play on TV</title>
<style>
body{font-family:system-ui,sans-serif;margin:0;padding:24px;background:#111;color:#eee}
form{max-width:480px;margin:20vh auto 0;display:grid;gap:12px}
input,button{box-sizing:border-box;width:100%;font:inherit;font-size:18px;padding:14px;border-radius:8px}
input{border:1px solid #555;background:#222;color:#fff}
button{border:0;font-weight:600;cursor:pointer}
</style>
<form method="post" action="/play">
<input type="url" name="url" placeholder="Paste a URL" inputmode="url" autocomplete="off" required autofocus>
<button type="submit">Play</button>
</form>
"#;
