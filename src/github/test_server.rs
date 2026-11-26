//! A local HTTP server for tests. It answers each connection with the next
//! scripted reply, closes the connection, and records what was asked.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

/// One request as the server received it.
#[derive(Debug, Clone)]
pub struct Recorded {
    pub path: String,
    /// Header names in lowercase.
    headers: BTreeMap<String, String>,
}

impl Recorded {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }
}

/// The raw bytes one connection receives before the server closes it.
#[derive(Debug, Clone)]
pub struct Reply(Vec<u8>);

impl Reply {
    /// A response with `status` (such as `200 OK`), `headers` and `body`. Every
    /// reply closes its connection, so each request arrives on a new one.
    pub fn new(status: &str, headers: &[(&str, String)], body: &[u8]) -> Self {
        let mut head = format!("HTTP/1.1 {status}\r\nConnection: close\r\n");
        for (name, value) in headers {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
        head.push_str("\r\n");
        let mut bytes = head.into_bytes();
        bytes.extend_from_slice(body);
        Reply(bytes)
    }

    /// `200 OK` with the whole `body`.
    pub fn full(body: &[u8], extra: &[(&str, String)]) -> Self {
        let mut headers = vec![("Content-Length", body.len().to_string())];
        headers.extend_from_slice(extra);
        Reply::new("200 OK", &headers, body)
    }

    /// `206 Partial Content` carrying `body` as the bytes from `start` of a
    /// file of `total` bytes.
    pub fn partial(body: &[u8], start: u64, total: u64, extra: &[(&str, String)]) -> Self {
        let end = start + body.len() as u64 - 1;
        let mut headers = vec![
            ("Content-Length", body.len().to_string()),
            ("Content-Range", format!("bytes {start}-{end}/{total}")),
        ];
        headers.extend_from_slice(extra);
        Reply::new("206 Partial Content", &headers, body)
    }

    /// `416 Range Not Satisfiable` for a file of `total` bytes.
    pub fn unsatisfiable(total: u64) -> Self {
        Reply::new(
            "416 Range Not Satisfiable",
            &[
                ("Content-Length", "0".to_string()),
                ("Content-Range", format!("bytes */{total}")),
            ],
            b"",
        )
    }

    /// `302 Found` to `location`, as GitHub answers a release download.
    pub fn redirect(location: &str) -> Self {
        Reply::new(
            "302 Found",
            &[
                ("Location", location.to_string()),
                ("Content-Length", "0".to_string()),
            ],
            b"",
        )
    }
}

/// A server on `127.0.0.1` that serves its replies in order, one per connection.
pub struct TestServer {
    addr: SocketAddr,
    requests: Arc<Mutex<Vec<Recorded>>>,
    worker: Option<JoinHandle<()>>,
}

impl TestServer {
    pub fn start(replies: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a local port");
        let addr = listener.local_addr().expect("read the local port");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&requests);
        let worker = thread::spawn(move || {
            for reply in replies {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let Some(request) = read_request(&mut stream) else {
                    return;
                };
                recorded.lock().unwrap().push(request);
                let _ = stream.write_all(&reply.0);
                let _ = stream.flush();
                let _ = stream.shutdown(Shutdown::Both);
            }
        });
        TestServer {
            addr,
            requests,
            worker: Some(worker),
        }
    }

    /// `http://127.0.0.1:<port><path>`.
    pub fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }

    /// The requests received so far, oldest first.
    pub fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for TestServer {
    /// An empty connection ends a worker that still waits for a request.
    fn drop(&mut self) {
        let Some(worker) = self.worker.take() else {
            return;
        };
        if !worker.is_finished() {
            let _ = TcpStream::connect(self.addr);
        }
        let _ = worker.join();
    }
}

/// Reads one request head; `None` when the connection closes first.
fn read_request(stream: &mut TcpStream) -> Option<Recorded> {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        match stream.read(&mut byte) {
            Ok(1) => head.push(byte[0]),
            _ => return None,
        }
    }
    let text = String::from_utf8_lossy(&head);
    let mut lines = text.lines();
    let path = lines.next()?.split_whitespace().nth(1)?.to_string();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
        .collect();
    Some(Recorded { path, headers })
}
