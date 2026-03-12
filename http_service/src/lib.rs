use std::future::Future;
use std::pin::Pin;

use bytes::Bytes;
use http_body_util::{BodyExt, Empty};
use hyper::Request;
use hyper::body::Incoming;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use tokio::net::UnixStream;
use tower::Service;

/// The path to the Unix domain socket.
const SOCKET_PATH: &str = "/run/container_launcher/teeserver.sock";

/// The URL to request over the Unix domain socket.
const REQUEST_URL: &str = "http://localhost/v1/token";

/// A connector that establishes connections over a Unix domain socket
/// instead of TCP. Hyper's client layer will call this connector
/// whenever it needs a new transport connection.
#[derive(Clone)]
struct UnixConnector {
    socket_path: String,
}

impl UnixConnector {
    fn new(socket_path: &str) -> Self {
        Self {
            socket_path: socket_path.to_string(),
        }
    }
}

/// Implement `tower::Service` for `UnixConnector` so it can be used
/// as a connector with `hyper_util::client::legacy::Client`.
impl Service<hyper::Uri> for UnixConnector {
    type Response = hyper_util::rt::TokioIo<UnixStream>;
    type Error = std::io::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, _uri: hyper::Uri) -> Self::Future {
        let socket_path = self.socket_path.clone();
        Box::pin(async move {
            let stream = UnixStream::connect(&socket_path).await?;
            Ok(hyper_util::rt::TokioIo::new(stream))
        })
    }
}

pub async fn connect_local() -> Result<(), Box<dyn std::error::Error>> {
    // Build a hyper client that uses our Unix domain socket connector.
    let connector = UnixConnector::new(SOCKET_PATH);
    let client: Client<UnixConnector, Empty<Bytes>> =
        Client::builder(TokioExecutor::new()).build(connector);

    // Build the HTTP request. The host in the URI is ignored by our
    // connector — all traffic is routed through the Unix socket — but
    // it is still sent in the HTTP `Host` header as required by HTTP/1.1.
    let request = Request::builder()
        .uri(REQUEST_URL)
        .header("Host", "localhost")
        .body(Empty::<Bytes>::new())?;

    println!("Sending GET request to {} via {}", REQUEST_URL, SOCKET_PATH);

    // Send the request and await the response.
    let response: hyper::Response<Incoming> = client.request(request).await?;

    let status = response.status();
    println!("Response status: {}", status);

    // Print response headers.
    println!("Response headers:");
    for (key, value) in response.headers() {
        println!("  {}: {}", key, value.to_str().unwrap_or("<non-utf8>"));
    }

    // Collect and print the response body.
    let body_bytes = response.into_body().collect().await?.to_bytes();
    let body_str = String::from_utf8_lossy(&body_bytes);
    println!("Response body:\n{}", body_str);

    Ok(())
}
