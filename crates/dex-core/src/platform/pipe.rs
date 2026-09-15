//! The named pipe the CLI and MCP server connect to (docs/prd.md §6.1–6.2).
//!
//! One pipe name per user; a three-line handshake; then one JSON request per
//! line, one response per line. Knows nothing about commands: every
//! authenticated request goes to the handler the caller supplies.
//!
//! Tasks: `serve` runs for the app's lifetime (it ends with the runtime); each
//! client gets a task that ends when the client disconnects.

use std::future::Future;
use std::io;
use std::time::Duration;

use dex_protocol::{
    AuthMessage, ErrorBody, ErrorCode, Hello, HelloMessage, PROTOCOL_VERSION, Request, Response,
};
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, Lines};
use tokio::net::windows::named_pipe::ServerOptions;
use tracing::Instrument;

pub use tokio::net::windows::named_pipe::NamedPipeServer;

use super::auth::{self, Token};

/// A client that has not finished the handshake by now is dropped.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// `dex-<username>`. Pipe names are machine-wide, so a fixed name would
/// collide between users. Must match the CLI's copy in `dex-cli/src/client.rs`.
pub fn default_name() -> String {
    let user = std::env::var("USERNAME").unwrap_or_default().to_lowercase();
    let safe: String = user
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("dex-{safe}")
}

/// The full pipe path for a name.
pub fn path(name: &str) -> String {
    format!(r"\\.\pipe\{name}")
}

/// Creates the first instance of the pipe. Fails if any process already holds
/// the name — which must stop the app from starting: serving next to an
/// impostor would let a stranger answer our clients (PRD §6.1).
pub fn bind(name: &str) -> io::Result<NamedPipeServer> {
    ServerOptions::new()
        .first_pipe_instance(true)
        .create(path(name))
}

/// Accepts clients until the runtime shuts down. Each authenticated request
/// is answered by `handler`.
pub async fn serve<H, F>(first: NamedPipeServer, name: String, token: Token, handler: H)
where
    H: Fn(Request) -> F + Clone + Send + Sync + 'static,
    F: Future<Output = Response> + Send + 'static,
{
    let mut listening = first;
    loop {
        if let Err(err) = listening.connect().await {
            tracing::warn!(%err, "pipe client failed to connect");
            continue;
        }
        // Create the next instance before serving this client, so a second
        // client never finds the name missing between the two.
        let next = match ServerOptions::new().create(path(&name)) {
            Ok(next) => next,
            Err(err) => {
                tracing::error!(%err, "cannot create the next pipe instance; no longer accepting clients");
                return;
            }
        };
        let client = std::mem::replace(&mut listening, next);
        let handler = handler.clone();
        let token = token.clone();
        tokio::spawn(
            async move {
                if let Err(err) = serve_client(client, &token, handler).await {
                    tracing::debug!(%err, "pipe client ended with an error");
                }
            }
            .instrument(tracing::debug_span!("pipe_client")),
        );
    }
}

async fn serve_client<S, H, F>(stream: S, token: &Token, handler: H) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite,
    H: Fn(Request) -> F,
    F: Future<Output = Response>,
{
    let (read, mut write) = tokio::io::split(stream);
    let mut lines = BufReader::new(read).lines();
    match tokio::time::timeout(HANDSHAKE_TIMEOUT, handshake(&mut lines, &mut write, token)).await {
        Ok(Ok(true)) => {}
        Ok(Ok(false)) | Err(_) => return Ok(()),
        Ok(Err(err)) => return Err(err),
    }
    while let Some(line) = lines.next_line().await? {
        let response = match serde_json::from_str::<Request>(&line) {
            Ok(request) => handler(request).await,
            Err(err) => Response::failure(
                "",
                ErrorBody {
                    code: ErrorCode::InvalidArgs,
                    message: format!("not a request: {err}"),
                    repair: r#"Send one JSON object per line: {"id":"1","cmd":"workspace.list","args":{}}."#.into(),
                },
            ),
        };
        write_line(&mut write, &response).await?;
    }
    Ok(())
}

/// The server's half of the handshake. False means the client is not trusted
/// (or gave up); the connection is then closed.
async fn handshake<R, W>(
    lines: &mut Lines<BufReader<R>>,
    write: &mut W,
    token: &Token,
) -> io::Result<bool>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let Some(line) = lines.next_line().await? else {
        return Ok(false);
    };
    let Ok(hello) = serde_json::from_str::<HelloMessage>(&line) else {
        return Ok(false);
    };
    let server_nonce = auth::nonce()?;
    let reply = HelloMessage {
        hello: Hello {
            version: PROTOCOL_VERSION.into(),
            nonce: server_nonce.clone(),
            proof: Some(auth::sign(token, &hello.hello.nonce)),
        },
    };
    write_line(write, &reply).await?;

    let Some(line) = lines.next_line().await? else {
        return Ok(false);
    };
    let accepted = serde_json::from_str::<AuthMessage>(&line)
        .is_ok_and(|auth| auth::verify(token, &server_nonce, &auth.auth));
    if !accepted {
        let refusal = Response::failure(
            "",
            ErrorBody {
                code: ErrorCode::Unauthorized,
                message: "the client could not prove it knows the Dex token".into(),
                repair: "Run `dex doctor`. If Dex was restarted, the old token is no longer valid; retry.".into(),
            },
        );
        write_line(write, &refusal).await?;
    }
    Ok(accepted)
}

async fn write_line<W: AsyncWrite + Unpin>(
    write: &mut W,
    message: &impl Serialize,
) -> io::Result<()> {
    let mut line = serde_json::to_vec(message).map_err(io::Error::other)?;
    line.push(b'\n');
    write.write_all(&line).await?;
    write.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipe_names_are_per_user_and_safe() {
        let name = default_name();
        assert!(name.starts_with("dex-"));
        assert!(
            name.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        );
        assert_eq!(path("dex-x"), r"\\.\pipe\dex-x");
    }
}
