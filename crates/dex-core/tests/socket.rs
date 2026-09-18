//! Integration tests on macOS and Linux: a real Unix socket, the real
//! handshake, a stub handler. The Windows twin is `pipe.rs`.
#![cfg(unix)]
// A test crate: panicking is how a test fails.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use dex_core::platform::auth::{self, Token};
use dex_core::platform::pipe;
use dex_protocol::{AuthMessage, HelloMessage, PROTOCOL_VERSION, Request, Response};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};

/// Starts a server on a fresh socket that echoes each command name back.
fn start(dir: &tempfile::TempDir, token: Token) -> PathBuf {
    let path = dir.path().join("dex-test.sock");
    let server = pipe::bind_at(&path).unwrap();
    tokio::spawn(pipe::serve(
        server,
        path.to_string_lossy().into_owned(),
        token,
        |request: Request| async move { Response::success(request.id, json!({ "echo": request.cmd })) },
    ));
    path
}

struct Conn {
    lines: Lines<BufReader<OwnedReadHalf>>,
    write: OwnedWriteHalf,
}

impl Conn {
    async fn open(path: &PathBuf) -> Self {
        let (read, write) = UnixStream::connect(path).await.unwrap().into_split();
        Self {
            lines: BufReader::new(read).lines(),
            write,
        }
    }

    async fn send(&mut self, value: Value) {
        let mut line = serde_json::to_vec(&value).unwrap();
        line.push(b'\n');
        self.write.write_all(&line).await.unwrap();
    }

    async fn next(&mut self) -> Option<Value> {
        self.lines
            .next_line()
            .await
            .unwrap()
            .map(|line| serde_json::from_str(&line).unwrap())
    }

    /// The client's half of the handshake.
    async fn authenticate(&mut self, token: &Token) {
        let nonce = auth::nonce().unwrap();
        self.send(json!({ "hello": { "version": PROTOCOL_VERSION, "nonce": nonce } }))
            .await;
        let reply: HelloMessage = serde_json::from_value(self.next().await.unwrap()).unwrap();
        assert!(auth::verify(
            token,
            &nonce,
            reply.hello.proof.as_deref().unwrap()
        ));
        let answer = AuthMessage {
            auth: auth::sign(token, &reply.hello.nonce),
        };
        self.send(serde_json::to_value(answer).unwrap()).await;
    }
}

#[tokio::test]
async fn a_client_that_knows_the_token_is_served() {
    let dir = tempfile::tempdir().unwrap();
    let token = Token::generate().unwrap();
    let path = start(&dir, token.clone());
    let mut conn = Conn::open(&path).await;
    conn.authenticate(&token).await;
    conn.send(json!({ "id": "1", "cmd": "workspace.list", "args": {} }))
        .await;
    let reply: Response = serde_json::from_value(conn.next().await.unwrap()).unwrap();
    assert!(reply.ok, "{reply:?}");
}

#[tokio::test]
async fn a_client_that_does_not_know_the_token_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = start(&dir, Token::generate().unwrap());
    let mut conn = Conn::open(&path).await;
    conn.authenticate_badly().await;
}

impl Conn {
    async fn authenticate_badly(&mut self) {
        let nonce = auth::nonce().unwrap();
        self.send(json!({ "hello": { "version": PROTOCOL_VERSION, "nonce": nonce } }))
            .await;
        let reply: HelloMessage = serde_json::from_value(self.next().await.unwrap()).unwrap();
        let wrong = Token::generate().unwrap();
        let answer = AuthMessage {
            auth: auth::sign(&wrong, &reply.hello.nonce),
        };
        self.send(serde_json::to_value(answer).unwrap()).await;
        let refusal: Response = serde_json::from_value(self.next().await.unwrap()).unwrap();
        assert_eq!(
            refusal.error.unwrap().code,
            dex_protocol::ErrorCode::Unauthorized
        );
        assert!(
            self.next().await.is_none(),
            "the connection is closed after a refusal"
        );
    }
}

#[tokio::test]
async fn only_the_owner_may_connect() {
    // Unix sockets are files: other users on the machine must not open it.
    let dir = tempfile::tempdir().unwrap();
    let path = start(&dir, Token::generate().unwrap());
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "{mode:o}");
}

#[tokio::test]
async fn a_second_dex_is_refused_but_a_stale_socket_from_a_crash_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let path = start(&dir, Token::generate().unwrap());
    // Another Dex is serving: binding the same socket must fail, not steal it.
    assert!(pipe::bind_at(&path).is_err());

    // A socket file nobody serves - left by a crash - is cleared and rebound.
    let stale_dir = tempfile::tempdir().unwrap();
    let stale = stale_dir.path().join("dex-stale.sock");
    drop(std::os::unix::net::UnixListener::bind(&stale).unwrap());
    assert!(stale.exists());
    assert!(pipe::bind_at(&stale).is_ok());
}

#[tokio::test]
async fn a_second_dex_is_refused_even_when_the_socket_file_has_gone() {
    // Two launches racing over a stale socket could both clear it; the lock
    // beside it is what decides, and the first holder keeps it.
    let dir = tempfile::tempdir().unwrap();
    let path = start(&dir, Token::generate().unwrap());
    std::fs::remove_file(&path).unwrap();
    let err = pipe::bind_at(&path).err().expect("the lock is held");
    assert_eq!(err.kind(), std::io::ErrorKind::AddrInUse, "{err}");
}

#[test]
fn a_socket_path_too_long_for_macos_is_refused_with_the_fix() {
    let dir = tempfile::tempdir().unwrap();
    let long = dir.path().join("x".repeat(120)).join("dex-me.sock");
    let err = pipe::bind_at(&long).err().expect("too long");
    assert!(err.to_string().contains("DEX_DATA_DIR"), "{err}");
}
