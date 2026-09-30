use eframe::egui;
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::mpsc::{self, Receiver, Sender},
    time::Duration,
};

use crate::input::Action;
use std::sync::{Arc, Mutex};

pub enum Command {
    Play(String),
    Action(Action),
}
pub struct Request {
    pub command: Command,
    pub reply: Sender<Result<(), String>>,
}
#[derive(Default, Clone)]
pub struct Status {
    pub active: bool,
    pub embedded: bool,
    pub loading: bool,
    pub paused: bool,
    pub position: Option<f64>,
    pub duration: Option<f64>,
    pub volume: f64,
    pub muted: bool,
    pub error: bool,
}
impl Status {
    fn json(&self) -> String {
        fn number(value: Option<f64>) -> String {
            value
                .filter(|v| v.is_finite())
                .map_or("null".into(), |v| v.to_string())
        }
        format!(
            r#"{{"active":{},"embedded":{},"loading":{},"paused":{},"position":{},"duration":{},"volume":{},"muted":{},"error":{}}}"#,
            self.active,
            self.embedded,
            self.loading,
            self.paused,
            number(self.position),
            number(self.duration),
            number(Some(self.volume)),
            self.muted,
            self.error
        )
    }
}
pub struct Server {
    requests: Receiver<Request>,
    status: Arc<Mutex<Status>>,
}
impl Server {
    pub fn new(address: SocketAddr, ctx: egui::Context) -> Self {
        let (send, requests) = mpsc::channel();
        let status = Arc::new(Mutex::new(Status::default()));
        let shared = status.clone();
        std::thread::spawn(move || serve(address, send, ctx, shared));
        Self { requests, status }
    }
    pub fn try_recv(&self) -> Option<Request> {
        self.requests.try_recv().ok()
    }
    pub fn publish(&self, status: Status) {
        *self.status.lock().unwrap() = status;
    }
}
fn serve(
    address: SocketAddr,
    requests: Sender<Request>,
    ctx: egui::Context,
    status: Arc<Mutex<Status>>,
) {
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
            Ok(mut stream) => handle(&mut stream, &requests, &ctx, &status),
            Err(e) => eprintln!("Web server connection failed: {e}"),
        }
    }
}
// A request can span TCP reads. Cap both header and body rather than accepting
// whatever happened to arrive in the first packet.
fn read_request(stream: &mut impl Read) -> Option<String> {
    let mut bytes = Vec::new();
    loop {
        if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            let header = std::str::from_utf8(&bytes[..end]).ok()?;
            let mut length = None;
            for line in header.lines().skip(1) {
                let (name, value) = line.split_once(':')?;
                if name.eq_ignore_ascii_case("transfer-encoding") {
                    return None;
                }
                if name.eq_ignore_ascii_case("content-length") {
                    if length.is_some() {
                        return None;
                    }
                    length = Some(value.trim().parse::<usize>().ok()?);
                }
            }
            let total = (end + 4).checked_add(length.unwrap_or(0))?;
            if total > 8192 {
                return None;
            }
            if bytes.len() >= total {
                return String::from_utf8(bytes[..total].to_vec()).ok();
            }
        }
        if bytes.len() >= 8192 {
            return None;
        }
        let mut buffer = [0; 1024];
        let size = stream.read(&mut buffer).ok()?;
        if size == 0 {
            return None;
        }
        bytes.extend_from_slice(&buffer[..size]);
    }
}
fn control(body: &str) -> Option<Action> {
    let value = body.split('&').find_map(|f| f.strip_prefix("action="))?;
    Some(match percent_decode(value)?.as_str() {
        "pause" => Action::PlayPause,
        "stop" => Action::Stop,
        "seek_back" => Action::Seek(-10),
        "seek_forward" => Action::Seek(10),
        "volume_down" => Action::Volume(-5),
        "volume_up" => Action::Volume(5),
        "mute" => Action::Mute,
        _ => return None,
    })
}
fn handle(
    stream: &mut TcpStream,
    requests: &Sender<Request>,
    ctx: &egui::Context,
    status: &Mutex<Status>,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let Some(request) = read_request(stream) else {
        respond(
            stream,
            "400 Bad Request",
            "text/plain",
            "Invalid or oversized request\n",
        );
        return;
    };
    let Some((header, body)) = request.split_once("\r\n\r\n") else {
        return;
    };
    let parts: Vec<_> = header
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .collect();
    if parts.len() != 3 {
        respond(stream, "400 Bad Request", "text/plain", "Bad request\n");
        return;
    }
    match (parts[0], parts[1]) {
        ("GET", "/") => {
            respond(stream, "200 OK", "text/html; charset=utf-8", PAGE);
            return;
        }
        ("GET", "/status") => {
            respond(
                stream,
                "200 OK",
                "application/json",
                &status.lock().unwrap().json(),
            );
            return;
        }
        _ => {}
    }
    let command = match (parts[0], parts[1]) {
        ("POST", "/play") => form_url(body).map(Command::Play),
        ("POST", "/control") => control(body).map(Command::Action),
        _ => {
            respond(stream, "404 Not Found", "text/plain", "Not found\n");
            return;
        }
    };
    let Some(command) = command else {
        respond(
            stream,
            "400 Bad Request",
            "text/plain",
            "Invalid URL or playback action\n",
        );
        return;
    };
    let (reply, result) = mpsc::channel();
    if requests.send(Request { command, reply }).is_err() {
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
        Ok(Err(error)) => respond(
            stream,
            if error == "Playback is already active" {
                "409 Conflict"
            } else {
                "502 Bad Gateway"
            },
            "text/plain; charset=utf-8",
            &format!("{error}\n"),
        ),
        Err(_) => respond(
            stream,
            "503 Service Unavailable",
            "text/plain",
            "Launcher unavailable\n",
        ),
    }
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
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
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
<title>Media Launcher</title>
<style>
body{font-family:system-ui,sans-serif;margin:24px;background:#111;color:#eee}
main{max-width:480px;margin:auto} form{display:grid;gap:12px}
input,button{box-sizing:border-box;font:inherit;padding:14px;border-radius:6px}
input{width:100%;border:1px solid #555;background:#222;color:#fff}
button{border:1px solid #555;background:#222;color:#fff;cursor:pointer}
button:disabled{opacity:.4} #controls{display:grid;grid-template-columns:1fr 1fr;gap:12px}
</style>
<main><h1>Media Launcher</h1><p id="status">Connecting…</p>
<form method="post" action="/play" id="play">
<input type="url" name="url" placeholder="Paste a URL" inputmode="url" autocomplete="off" required>
<button type="submit">Play URL</button></form>
<p id="message" role="status"></p>
<div id="controls">
<button data-action="pause">Play / pause</button><button data-action="stop">Stop</button>
<button data-action="seek_back">−10 seconds</button><button data-action="seek_forward">+10 seconds</button>
<button data-action="volume_down">Volume −</button><button data-action="volume_up">Volume +</button>
<button data-action="mute">Mute / unmute</button></div></main>
<script>
const status=document.querySelector('#status'), message=document.querySelector('#message');
const buttons=[...document.querySelectorAll('[data-action]')];
function time(v){if(v===null)return 'Live';return Math.floor(v/60)+':'+String(Math.floor(v%60)).padStart(2,'0')}
async function refresh(){try{const r=await fetch('/status');if(!r.ok)throw Error('Disconnected');
 const s=await r.json();status.textContent=s.error?'Playback error — see TV':!s.active?'Ready for media':!s.embedded?'Playing in external mpv':s.loading?'Loading…':(s.paused?'Paused':'Playing')+' · '+time(s.position)+' / '+time(s.duration)+' · Volume '+Math.round(s.volume)+(s.muted?' (muted)':'');
 buttons.forEach(b=>b.disabled=!s.active||!s.embedded);
}catch(e){status.textContent='Launcher unavailable';buttons.forEach(b=>b.disabled=true)}}
async function send(path,body){message.textContent='';try{const r=await fetch(path,{method:'POST',body});
 if(!r.ok)throw Error(await r.text()); await refresh();}catch(e){message.textContent=e.message}}
document.querySelector('#play').addEventListener('submit',e=>{e.preventDefault();send('/play',new URLSearchParams(new FormData(e.target)))});
buttons.forEach(b=>b.addEventListener('click',()=>send('/control',new URLSearchParams({action:b.dataset.action}))));
refresh();setInterval(refresh,1000);
</script>
"#;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn controls_are_whitelisted_and_urls_remain_literal() {
        assert_eq!(control("action=seek_back"), Some(Action::Seek(-10)));
        assert_eq!(control("action=quit"), None);
        assert_eq!(
            form_url("url=https%3A%2F%2Fexample.com%2Fx%3Fa%3D1%26b%3D2"),
            Some("https://example.com/x?a=1&b=2".into())
        );
    }
    #[test]
    fn fragmented_requests_are_read_completely_and_lengths_are_bounded() {
        struct Fragmented(std::io::Cursor<Vec<u8>>);
        impl Read for Fragmented {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                self.0.read(&mut buffer[..1])
            }
        }
        let request = "POST /control HTTP/1.1\r\nContent-Length: 12\r\n\r\naction=pause";
        assert_eq!(
            read_request(&mut Fragmented(std::io::Cursor::new(
                request.as_bytes().to_vec()
            ))),
            Some(request.into())
        );
        assert!(
            read_request(&mut std::io::Cursor::new(
                b"POST /play HTTP/1.1\r\nContent-Length: 9000\r\n\r\n"
            ))
            .is_none()
        );
    }
}
