//! The pipe client: connect, prove both sides know the token, send requests.
//!
//! A Windows named pipe, or a Unix socket on macOS and Linux (`paths.rs` says
//! which). Synchronous on purpose: both open like files, and not starting an
//! async runtime keeps every hook invocation fast (PRD §9.3).
//! The HMAC helpers duplicate `dex_core::platform::auth` so the CLI stays free
//! of the daemon's dependencies; both are tested against RFC 4231's vector.

#[cfg(windows)]
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
#[cfg(windows)]
use std::thread;
#[cfg(windows)]
use std::time::{Duration, Instant};

use dex_protocol::{
    AuthMessage, ErrorBody, ErrorCode, Hello, HelloMessage, PROTOCOL_VERSION, Request, Response,
};
use hmac::{Hmac, KeyInit, Mac};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sha2::Sha256;

use crate::paths::{self, Address};

/// Windows' ERROR_PIPE_BUSY: every pipe instance is taken for the moment.
#[cfg(windows)]
const ERROR_PIPE_BUSY: i32 = 231;
/// How long to keep retrying a busy pipe before giving up.
#[cfg(windows)]
const BUSY_WAIT: Duration = Duration::from_secs(2);

/// An authenticated connection to the running app.
pub struct Client {
    reader: BufReader<Box<dyn Read + Send>>,
    writer: Box<dyn Write + Send>,
    next_id: u64,
    app_version: String,
}

/// Where the app is, as `dex doctor` names it.
pub fn describe_address() -> String {
    paths::address().map_or_else(|| "no address".to_owned(), |address| address.describe())
}

/// Connects to the app and completes the handshake.
pub fn connect() -> Result<Client, ErrorBody> {
    let address = paths::address().ok_or_else(not_running)?;
    let (reader, writer) = open(&address)?;
    let mut client = Client {
        reader: BufReader::new(reader),
        writer,
        next_id: 0,
        app_version: String::new(),
    };
    client.handshake()?;
    Ok(client)
}

type Halves = (Box<dyn Read + Send>, Box<dyn Write + Send>);

fn open(address: &Address) -> Result<Halves, ErrorBody> {
    match address {
        #[cfg(windows)]
        Address::Pipe(path) => {
            let file = open_pipe(path)?;
            let writer = file.try_clone().map_err(|err| pipe_error(&err))?;
            Ok((Box::new(file), Box::new(writer)))
        }
        #[cfg(unix)]
        Address::Unix(path) => {
            let stream = std::os::unix::net::UnixStream::connect(path).map_err(|err| {
                if matches!(
                    err.kind(),
                    ErrorKind::NotFound | ErrorKind::ConnectionRefused
                ) {
                    not_running()
                } else {
                    pipe_error(&err)
                }
            })?;
            let writer = stream.try_clone().map_err(|err| pipe_error(&err))?;
            Ok((Box::new(stream), Box::new(writer)))
        }
        #[allow(unreachable_patterns)]
        _ => Err(not_running()),
    }
}

#[cfg(windows)]
fn open_pipe(path: &str) -> Result<std::fs::File, ErrorBody> {
    let deadline = Instant::now() + BUSY_WAIT;
    loop {
        match OpenOptions::new().read(true).write(true).open(path) {
            Ok(file) => return Ok(file),
            Err(err)
                if err.raw_os_error() == Some(ERROR_PIPE_BUSY) && Instant::now() < deadline =>
            {
                thread::sleep(Duration::from_millis(20));
            }
            Err(err) if err.kind() == ErrorKind::NotFound => return Err(not_running()),
            Err(err) => return Err(pipe_error(&err)),
        }
    }
}

impl Client {
    /// The app's protocol version, as it reported during the handshake.
    pub fn app_version(&self) -> &str {
        &self.app_version
    }

    fn handshake(&mut self) -> Result<(), ErrorBody> {
        let token = read_token()?;
        let nonce = random_hex()?;
        self.send(&HelloMessage {
            hello: Hello {
                version: PROTOCOL_VERSION.into(),
                nonce: nonce.clone(),
                proof: None,
            },
        })?;
        let reply = self.receive()?;
        let Ok(hello) = serde_json::from_value::<HelloMessage>(reply.clone()) else {
            return Err(refusal(reply));
        };
        // Check the app's proof before sending anything that matters: a
        // process squatting on the pipe name cannot produce it.
        let proof = hello.hello.proof.unwrap_or_default();
        if !verify(&token, &nonce, &proof) {
            return Err(unauthorized(
                "the process answering on Dex's pipe could not prove it is Dex",
            ));
        }
        self.app_version = hello.hello.version;
        self.send(&AuthMessage {
            auth: sign(&token, &hello.hello.nonce),
        })
    }

    /// Runs one command and decodes its result.
    pub fn call<T: DeserializeOwned>(&mut self, cmd: &str, args: Value) -> Result<T, ErrorBody> {
        if self.app_version != PROTOCOL_VERSION {
            return Err(version_mismatch(&self.app_version));
        }
        self.next_id += 1;
        self.send(&Request {
            id: format!("cli-{}", self.next_id),
            cmd: cmd.into(),
            args,
        })?;
        let response: Response = serde_json::from_value(self.receive()?)
            .map_err(|err| internal(format!("unreadable reply: {err}")))?;
        if !response.ok {
            return Err(response
                .error
                .unwrap_or_else(|| internal("the app reported a failure without details".into())));
        }
        serde_json::from_value(response.data.unwrap_or(Value::Null))
            .map_err(|err| internal(format!("unexpected reply: {err}")))
    }

    fn send(&mut self, message: &impl Serialize) -> Result<(), ErrorBody> {
        let mut line = serde_json::to_vec(message).map_err(|err| internal(err.to_string()))?;
        line.push(b'\n');
        self.writer.write_all(&line).map_err(|err| pipe_error(&err))
    }

    fn receive(&mut self) -> Result<Value, ErrorBody> {
        let mut line = String::new();
        let read = self
            .reader
            .read_line(&mut line)
            .map_err(|err| pipe_error(&err))?;
        if read == 0 {
            return Err(pipe_error(&std::io::Error::new(
                ErrorKind::UnexpectedEof,
                "the app closed the connection",
            )));
        }
        serde_json::from_str(&line).map_err(|err| internal(format!("unreadable reply: {err}")))
    }
}

/// A handshake reply that is not a hello: the app refused us, with a reason.
fn refusal(reply: Value) -> ErrorBody {
    serde_json::from_value::<Response>(reply)
        .ok()
        .and_then(|response| response.error)
        .unwrap_or_else(|| unauthorized("the app refused the connection"))
}

fn read_token() -> Result<Vec<u8>, ErrorBody> {
    let text = paths::data_dir()
        .map(|dir| dir.join("token"))
        .and_then(|path| std::fs::read_to_string(path).ok())
        .ok_or_else(|| ErrorBody {
            code: ErrorCode::NotRunning,
            message: "Dex's token file is missing".into(),
            repair: "Start the Dex app as this user; it writes a fresh token in its data folder at launch."
                .into(),
        })?;
    from_hex(text.trim()).ok_or_else(|| unauthorized("Dex's token file is corrupt"))
}

fn not_running() -> ErrorBody {
    ErrorBody {
        code: ErrorCode::NotRunning,
        message: "Dex is not running".into(),
        repair: "Start the Dex app, then run the command again.".into(),
    }
}

fn unauthorized(message: &str) -> ErrorBody {
    ErrorBody {
        code: ErrorCode::Unauthorized,
        message: message.into(),
        repair: "Restart Dex and retry. If it keeps happening, run `dex doctor`: something else may be using Dex's pipe name.".into(),
    }
}

fn pipe_error(err: &std::io::Error) -> ErrorBody {
    ErrorBody {
        code: ErrorCode::Internal,
        message: format!("pipe error: {err}"),
        repair: "Retry; if it persists, restart Dex and run `dex doctor`.".into(),
    }
}

fn internal(message: String) -> ErrorBody {
    ErrorBody {
        code: ErrorCode::Internal,
        message,
        repair: "This is a bug in Dex; please report it with the command you ran.".into(),
    }
}

fn version_mismatch(app: &str) -> ErrorBody {
    ErrorBody {
        code: ErrorCode::Internal,
        message: format!(
            "this dex CLI is version {PROTOCOL_VERSION}, but the running app is {app}"
        ),
        repair: "Install the CLI and the app from the same build.".into(),
    }
}

fn sign(key: &[u8], message: &str) -> String {
    // HMAC accepts keys of any length, so this cannot fail; an empty proof
    // would simply fail verification on the other side.
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(key) else {
        return String::new();
    };
    mac.update(message.as_bytes());
    to_hex(&mac.finalize().into_bytes())
}

fn verify(key: &[u8], message: &str, proof: &str) -> bool {
    let Some(expected) = from_hex(proof) else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(key) else {
        return false;
    };
    mac.update(message.as_bytes());
    mac.verify_slice(&expected).is_ok()
}

fn random_hex() -> Result<String, ErrorBody> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|err| internal(format!("no randomness available: {err}")))?;
    Ok(to_hex(&bytes))
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_matches_rfc_4231_test_case_1() {
        // The daemon's copy (dex_core::platform::auth) is checked against the same vector.
        assert_eq!(
            sign(&[0x0b; 20], "Hi There"),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
        let proof = sign(b"key", "msg");
        assert!(verify(b"key", "msg", &proof));
        assert!(!verify(b"other", "msg", &proof));
    }

    #[test]
    fn hex_round_trips_and_rejects_junk() {
        assert_eq!(from_hex(&to_hex(&[0, 15, 255])), Some(vec![0, 15, 255]));
        assert_eq!(from_hex("abc"), None);
        assert_eq!(from_hex("zz"), None);
    }
}
