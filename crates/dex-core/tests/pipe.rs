//! Integration tests: a real named pipe, the real handshake, a stub handler.
//! The macOS and Linux twin is `socket.rs`.
#![cfg(windows)]
// A test crate: panicking is how a test fails.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use dex_core::platform::auth::{self, Token};
use dex_core::platform::pipe;
use dex_protocol::{AuthMessage, HelloMessage, PROTOCOL_VERSION, Request, Response};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines, ReadHalf, WriteHalf};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};

/// Windows' ERROR_PIPE_BUSY.
const PIPE_BUSY: i32 = 231;

/// Starts a server on a fresh pipe name that echoes each command name back.
fn start(token: Token) -> String {
    let name = format!("dex-test-{}", uuid::Uuid::new_v4());
    let server = pipe::bind(&name).unwrap();
    tokio::spawn(pipe::serve(
        server,
        name.clone(),
        token,
        |request: Request| async move { Response::success(request.id, json!({ "echo": request.cmd })) },
    ));
    name
}

struct Conn {
    lines: Lines<BufReader<ReadHalf<NamedPipeClient>>>,
    write: WriteHalf<NamedPipeClient>,
}

impl Conn {
    async fn open(name: &str) -> Self {
        let client = loop {
            match ClientOptions::new().open(pipe::path(name)) {
                Ok(client) => break client,
                Err(err) if err.raw_os_error() == Some(PIPE_BUSY) => {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                Err(err) => panic!("cannot open the pipe: {err}"),
            }
        };
        let (read, write) = tokio::io::split(client);
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

    async fn send_raw(&mut self, line: &str) {
        self.write
            .write_all(format!("{line}\n").as_bytes())
            .await
            .unwrap();
    }

    async fn recv(&mut self) -> Option<Value> {
        let line = self.lines.next_line().await.unwrap()?;
        Some(serde_json::from_str(&line).unwrap())
    }

    /// The client half of the handshake, signing with `token`. Returns whether
    /// the server's proof checked out.
    async fn handshake(&mut self, token: &Token) -> bool {
        let nonce = auth::nonce().unwrap();
        self.send(json!({ "hello": { "version": PROTOCOL_VERSION, "nonce": nonce } }))
            .await;
        let reply: HelloMessage = serde_json::from_value(self.recv().await.unwrap()).unwrap();
        let genuine = auth::verify(token, &nonce, reply.hello.proof.as_deref().unwrap_or(""));
        let answer = AuthMessage {
            auth: auth::sign(token, &reply.hello.nonce),
        };
        self.send(serde_json::to_value(answer).unwrap()).await;
        genuine
    }
}

#[tokio::test]
async fn after_the_handshake_requests_and_responses_flow() {
    let token = Token::generate().unwrap();
    let name = start(token.clone());
    let mut conn = Conn::open(&name).await;

    assert!(
        conn.handshake(&token).await,
        "the server's proof must verify"
    );
    for (id, cmd) in [("1", "workspace.list"), ("2", "pane.list")] {
        conn.send(json!({ "id": id, "cmd": cmd, "args": {} })).await;
        let response: Response = serde_json::from_value(conn.recv().await.unwrap()).unwrap();
        assert!(response.ok);
        assert_eq!(response.id, id);
        assert_eq!(response.data.unwrap()["echo"], cmd);
    }
}

#[tokio::test]
async fn a_client_with_the_wrong_token_is_refused_and_disconnected() {
    let name = start(Token::generate().unwrap());
    let impostor = Token::generate().unwrap();
    let mut conn = Conn::open(&name).await;

    assert!(
        !conn.handshake(&impostor).await,
        "a proof made with another token must not verify"
    );
    let refusal: Response = serde_json::from_value(conn.recv().await.unwrap()).unwrap();
    assert_eq!(
        refusal.error.unwrap().code,
        dex_protocol::ErrorCode::Unauthorized
    );
    assert!(
        conn.recv().await.is_none(),
        "the server hangs up after refusing"
    );
}

#[tokio::test]
async fn a_malformed_line_gets_an_error_and_the_connection_stays_usable() {
    let token = Token::generate().unwrap();
    let name = start(token.clone());
    let mut conn = Conn::open(&name).await;
    assert!(conn.handshake(&token).await);

    conn.send_raw("this is not json").await;
    let error: Response = serde_json::from_value(conn.recv().await.unwrap()).unwrap();
    assert_eq!(
        error.error.unwrap().code,
        dex_protocol::ErrorCode::InvalidArgs
    );

    conn.send(json!({ "id": "3", "cmd": "still.here", "args": {} }))
        .await;
    let response: Response = serde_json::from_value(conn.recv().await.unwrap()).unwrap();
    assert!(response.ok);
}

#[tokio::test]
async fn a_pipe_name_already_held_cannot_be_bound_again() {
    let name = format!("dex-test-{}", uuid::Uuid::new_v4());
    let _first = pipe::bind(&name).unwrap();
    assert!(
        pipe::bind(&name).is_err(),
        "a second server must not share the name"
    );
}
