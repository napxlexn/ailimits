// providers/test_server.rs — a tiny local HTTP/1.1 server for the provider
// tests: scripted answers per path, every request recorded. Plain http on
// 127.0.0.1, so a test fetch never leaves the machine; providers reach it
// through `Net::local`, which swaps the scheme and host of their real URLs.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// A throwaway folder standing in for a tool's home, with made-up files in
/// it; removed on drop. No test ever reads a real tool folder.
pub(crate) struct TempHome(PathBuf);

impl TempHome {
    pub(crate) fn new(name: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "ailimits-provider-test-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir).expect("a temp folder");
        Self(dir)
    }

    pub(crate) fn path(&self) -> PathBuf {
        self.0.clone()
    }

    /// Write `content` to `relative` under the folder.
    pub(crate) fn write(&self, relative: &str, content: &str) {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("a temp sub-folder");
        }
        std::fs::write(path, content).expect("a temp file");
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// One scripted answer.
#[derive(Clone, Debug)]
pub(crate) enum Reply {
    /// A status and a JSON body.
    Status(u16, String),
    /// The connection closed without an answer: a transport error.
    Hangup,
}

impl Reply {
    pub(crate) fn ok(body: &str) -> Self {
        Reply::Status(200, body.to_string())
    }

    pub(crate) fn status(code: u16) -> Self {
        Reply::Status(code, String::new())
    }
}

/// One request the server received.
#[derive(Clone, Debug)]
pub(crate) struct Seen {
    /// The path, without the query.
    pub path: String,
    /// The Authorization header, when one came.
    pub auth: Option<String>,
}

#[derive(Default)]
struct State {
    /// (path suffix, answers in order; the last one repeats).
    routes: Vec<(String, VecDeque<Reply>)>,
    seen: Vec<Seen>,
}

pub(crate) struct TestServer {
    /// "http://127.0.0.1:port".
    pub base: String,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl TestServer {
    pub(crate) fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a local port");
        listener.set_nonblocking(true).expect("non-blocking accept");
        let base = format!("http://{}", listener.local_addr().expect("local address"));
        let state = Arc::new(Mutex::new(State::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (state, stop) = (state.clone(), stop.clone());
            std::thread::spawn(move || {
                while !stop.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((stream, _)) => serve(stream, &state),
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(2));
                        }
                        Err(_) => break,
                    }
                }
            })
        };
        Self {
            base,
            state,
            stop,
            thread: Some(thread),
        }
    }

    /// Answer the requests whose path ends with `suffix` with `replies`, in
    /// order; the last one repeats. A path without a route answers 404.
    pub(crate) fn route(&self, suffix: &str, replies: Vec<Reply>) {
        let mut state = self.state.lock().unwrap();
        state.routes.retain(|(s, _)| s != suffix);
        state.routes.push((suffix.to_string(), replies.into()));
    }

    /// Every request so far, in order.
    pub(crate) fn seen(&self) -> Vec<Seen> {
        self.state.lock().unwrap().seen.clone()
    }

    /// How many requests went to paths ending with `suffix`.
    pub(crate) fn count(&self, suffix: &str) -> usize {
        self.seen()
            .iter()
            .filter(|s| s.path.ends_with(suffix))
            .count()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(mut stream: TcpStream, state: &Mutex<State>) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let Some((path, auth, _body)) = read_request(&mut stream) else {
        return;
    };
    let reply = {
        let mut state = state.lock().unwrap();
        state.seen.push(Seen {
            path: path.clone(),
            auth,
        });
        let route = state
            .routes
            .iter_mut()
            .find(|(suffix, _)| path.ends_with(suffix.as_str()));
        match route {
            Some((_, replies)) if replies.len() > 1 => replies.pop_front(),
            Some((_, replies)) => replies.front().cloned(),
            None => None,
        }
        .unwrap_or(Reply::Status(404, String::new()))
    };
    match reply {
        Reply::Status(code, body) => {
            let head = format!(
                "HTTP/1.1 {code} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(body.as_bytes());
            let _ = stream.flush();
        }
        Reply::Hangup => {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    }
}

/// The request's path (without the query), its Authorization header and
/// its body.
fn read_request(stream: &mut TcpStream) -> Option<(String, Option<String>, Vec<u8>)> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = head.lines();
    let target = lines.next()?.split_whitespace().nth(1)?.to_string();
    let path = target.split('?').next().unwrap_or_default().to_string();
    let mut auth = None;
    let mut length = 0usize;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            let value = value.trim();
            if name.eq_ignore_ascii_case("authorization") {
                auth = Some(value.to_string());
            } else if name.eq_ignore_ascii_case("content-length") {
                length = value.parse().unwrap_or(0);
            }
        }
    }
    let mut body = buf[head_end..].to_vec();
    while body.len() < length {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    Some((path, auth, body))
}
