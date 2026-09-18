//! The local socket the CLI and MCP server connect to (docs/prd.md §6.1–6.2):
//! a named pipe on Windows, a Unix domain socket on macOS and Linux.
//!
//! One address per user; a three-line handshake; then one JSON request per
//! line, one response per line. Knows nothing about commands: every
//! authenticated request goes to the handler the caller supplies. Everything
//! after the connection is accepted is the same on every platform.
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
use tracing::Instrument;

#[cfg(windows)]
pub use windows::{Server, bind, path, serve, socket_env};

#[cfg(unix)]
pub use unix::{Server, bind, bind_at, path, serve, socket_env};

use super::auth::{self, Token};

/// A client that has not finished the handshake by now is dropped.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// `dex-<username>`. Pipe names are machine-wide, so a fixed name would
/// collide between users; on macOS and Linux the socket also lives in the
/// user's own data folder. Must match the CLI's copy in `dex-cli/src/client.rs`.
pub fn default_name() -> String {
    let user_var = if cfg!(windows) { "USERNAME" } else { "USER" };
    let user = std::env::var(user_var).unwrap_or_default().to_lowercase();
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

/// Serves one accepted connection, whatever it runs over.
fn spawn_client<S, H, F>(stream: S, token: Token, handler: H)
where
    S: AsyncRead + AsyncWrite + Send + 'static,
    H: Fn(Request) -> F + Clone + Send + Sync + 'static,
    F: Future<Output = Response> + Send + 'static,
{
    tokio::spawn(
        async move {
            if let Err(err) = serve_client(stream, &token, handler).await {
                tracing::debug!(%err, "client ended with an error");
            }
        }
        .instrument(tracing::debug_span!("pipe_client")),
    );
}

#[cfg(windows)]
mod windows {
    use std::future::Future;
    use std::io;

    use dex_protocol::{Request, Response};
    use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};

    use super::spawn_client;
    use crate::platform::auth::Token;

    /// A listening pipe instance.
    pub type Server = NamedPipeServer;

    /// The full pipe path for a name.
    pub fn path(name: &str) -> String {
        format!(r"\\.\pipe\{name}")
    }

    /// How a pane's shell is told where Dex is: `DEX_SOCKET=pipe:<name>`.
    pub fn socket_env(name: &str) -> io::Result<String> {
        Ok(format!("pipe:{name}"))
    }

    /// Creates the first instance of the pipe. Fails if any process already holds
    /// the name — which must stop the app from starting: serving next to an
    /// impostor would let a stranger answer our clients (PRD §6.1).
    pub fn bind(name: &str) -> io::Result<Server> {
        ServerOptions::new()
            .first_pipe_instance(true)
            .create(path(name))
    }

    /// Accepts clients until the runtime shuts down. Each authenticated request
    /// is answered by `handler`.
    pub async fn serve<H, F>(first: Server, name: String, token: Token, handler: H)
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
            spawn_client(client, token.clone(), handler.clone());
        }
    }
}

#[cfg(unix)]
mod unix {
    use std::fs::{File, OpenOptions};
    use std::future::Future;
    use std::io;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    use std::path::{Path, PathBuf};

    use dex_protocol::{Request, Response};
    use tokio::net::UnixListener;

    use super::spawn_client;
    use crate::platform::auth::Token;
    use crate::platform::paths;

    /// The listening socket, and the lock that says it is this process's.
    pub struct Server {
        listener: UnixListener,
        _lock: File,
    }

    /// The longest socket path macOS takes (`sun_path` is 104 bytes, with a NUL).
    const MAX_PATH: usize = 103;

    /// The socket for a name: `<name>.sock` in the data folder, beside the token.
    pub fn path(name: &str) -> io::Result<PathBuf> {
        Ok(paths::app_data_dir()?.join(format!("{name}.sock")))
    }

    /// How a pane's shell is told where Dex is: `DEX_SOCKET=unix:<path>`.
    pub fn socket_env(name: &str) -> io::Result<String> {
        Ok(format!("unix:{}", path(name)?.display()))
    }

    /// Binds the socket for `name` (see `bind_at`).
    pub fn bind(name: &str) -> io::Result<Server> {
        bind_at(&path(name)?)
    }

    /// Binds a socket at `at`, readable and writable by its owner only. Fails if
    /// another process is serving there - serving beside it would let a stranger
    /// answer our clients (PRD §6.1). A socket file nobody answers on, left by a
    /// crash, is removed first. An exclusive lock on `<name>.lock` beside it,
    /// held for the life of the server, is what makes that safe: two copies
    /// starting together cannot both decide the socket is stale.
    pub fn bind_at(at: &Path) -> io::Result<Server> {
        let length = at.as_os_str().len();
        if length > MAX_PATH {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "the socket path {} is {length} bytes; macOS allows {MAX_PATH}. Set DEX_DATA_DIR to a shorter folder.",
                    at.display()
                ),
            ));
        }
        let lock_path = at.with_extension("lock");
        let lock = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .mode(0o600)
            .open(&lock_path)?;
        if lock.try_lock().is_err() {
            return Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                format!("another Dex holds {}", lock_path.display()),
            ));
        }
        if at.exists() {
            if std::os::unix::net::UnixStream::connect(at).is_ok() {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    format!("another process is serving {}", at.display()),
                ));
            }
            std::fs::remove_file(at)?;
        }
        let listener = UnixListener::bind(at)?;
        std::fs::set_permissions(at, std::fs::Permissions::from_mode(0o600))?;
        Ok(Server {
            listener,
            _lock: lock,
        })
    }

    /// Accepts clients until the runtime shuts down. Each authenticated request
    /// is answered by `handler`.
    pub async fn serve<H, F>(listener: Server, name: String, token: Token, handler: H)
    where
        H: Fn(Request) -> F + Clone + Send + Sync + 'static,
        F: Future<Output = Response> + Send + 'static,
    {
        loop {
            match listener.listener.accept().await {
                Ok((stream, _)) => spawn_client(stream, token.clone(), handler.clone()),
                Err(err) => tracing::warn!(%err, socket = %name, "socket client failed to connect"),
            }
        }
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
    }

    #[cfg(windows)]
    #[test]
    fn a_pipe_name_is_a_path_under_the_pipe_namespace() {
        assert_eq!(path("dex-x"), r"\\.\pipe\dex-x");
        assert_eq!(socket_env("dex-x").unwrap(), "pipe:dex-x");
    }
}
