//! E2E harness: build + start the real `host` binary against the compose stack,
//! then hand out clients that reach it over HTTP or NATS.
//!
//! ```ignore
//! let host = Host::start("greet").await;
//! let deps = host.http_deps();          // or host.nats_deps().await
//! let client = GreetServiceClient::new(deps.channel(), deps.client_config());
//! ```

pub mod fake_openai;

use std::io::{BufRead, BufReader};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Once, mpsc};
use std::time::Duration;

use connectrpc::client::{ClientConfig, HttpClient};
use kernel::{Channel, Deps};
use platform::nats::NatsTransport;

const STARTUP_TIMEOUT: Duration = Duration::from_secs(20);

/// NATS from `podman compose up -d` (override with `E2E_NATS_URL`).
pub fn nats_url() -> String {
    std::env::var("E2E_NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into())
}

/// A running `host` process. Killed when dropped.
pub struct Host {
    child: Child,
    addr: String,
    nats_prefix: String,
}

impl Host {
    /// Start `host` with `SERVICES=<services>` on HTTP (free port) and NATS (unique
    /// subject prefix, so tests never collide with dev servers or each other).
    pub async fn start(services: &str) -> Host {
        Self::start_with_env(services, &[]).await
    }

    /// Like `start`, with extra environment variables for the services (e.g. a driver setting).
    pub async fn start_with_env(services: &str, env: &[(&str, &str)]) -> Host {
        require_compose_stack();
        build_host_once();
        let addr = format!("127.0.0.1:{}", free_port());
        let nats_prefix = format!("e2e-{}", uuid::Uuid::new_v4().simple());
        let mut child = Command::new(host_binary())
            .env("SERVICES", services)
            .env("ADDR", &addr)
            .env("NATS_URL", nats_url())
            .env("NATS_PREFIX", &nats_prefix)
            .envs(env.iter().copied())
            .stdout(Stdio::piped())
            .spawn()
            .expect("start host binary");
        wait_until_ready(&mut child);
        Host { child, addr, nats_prefix }
    }

    /// The NATS subject prefix (= instance id) of this host, for a service that calls it by id.
    pub fn nats_prefix(&self) -> &str {
        &self.nats_prefix
    }

    /// Deps whose channel calls this host over HTTP (Connect protocol).
    pub fn http_deps(&self) -> Deps {
        let base = format!("http://{}", self.addr).parse().expect("valid uri");
        Deps::new(Channel::new(HttpClient::plaintext()), ClientConfig::new(base))
    }

    /// Deps whose channel calls this host over NATS request/reply.
    pub async fn nats_deps(&self) -> Deps {
        let nc = async_nats::connect(nats_url()).await.expect("connect NATS");
        let channel = Channel::new(NatsTransport::new(nc, self.nats_prefix.clone()));
        Deps::new(channel, ClientConfig::new("http://nats".parse().expect("valid uri")))
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Fail fast with a clear message instead of a timeout deep inside a test.
fn require_compose_stack() {
    let url = nats_url();
    let host_port = url.trim_start_matches("nats://");
    if TcpStream::connect(host_port).is_err() {
        panic!("E2E needs NATS at {url}. Start the stack: `podman compose up -d`");
    }
}

/// Build the `host` binary once per test process, so e2e always runs the current code.
fn build_host_once() {
    static BUILD: Once = Once::new();
    BUILD.call_once(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let status =
            Command::new(cargo).args(["build", "--quiet", "--package", "host"]).status().expect("run cargo build");
        assert!(status.success(), "cargo build -p host failed");
    });
}

fn host_binary() -> PathBuf {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target = std::env::var("CARGO_TARGET_DIR").map(PathBuf::from).unwrap_or_else(|_| workspace.join("target"));
    target.join("debug").join("host")
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a free port");
    listener.local_addr().expect("local addr").port()
}

/// Wait until the host logs that HTTP listens and the NATS micro service started.
/// Every host log line is copied to stderr, so a failing test shows them.
fn wait_until_ready(child: &mut Child) {
    let stdout = child.stdout.take().expect("piped stdout");
    let (ready_tx, ready_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let (mut http, mut nats) = (false, false);
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            eprintln!("[host] {line}");
            http |= line.contains("http: listening");
            nats |= line.contains("nats: micro service started");
            if http && nats {
                let _ = ready_tx.send(());
            }
        }
    });
    if ready_rx.recv_timeout(STARTUP_TIMEOUT).is_err() {
        let _ = child.kill();
        panic!("host did not become ready within {STARTUP_TIMEOUT:?}; see [host] logs above");
    }
}
