use serde_bytes::ByteBuf;
use serde_cbor::Value;
use std::{error::Error, process::ExitCode};

use axum::{
    Router,
    body::Bytes,
    extract::ws::{Message, Utf8Bytes, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::any,
};
use axum_extra::TypedHeader;

use std::ops::ControlFlow;
use std::{net::SocketAddr, path::PathBuf};
use tower_http::{
    services::ServeDir,
    trace::{DefaultMakeSpan, TraceLayer},
};

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

//allows to extract the IP of connecting user
use axum::extract::connect_info::ConnectInfo;
use axum::extract::ws::CloseFrame;

//allows to split the websocket stream into separate TX and RX branches
use futures_util::{sink::SinkExt, stream::StreamExt};

#[tokio::main]
async fn main() -> eyre::Result<ExitCode> {
    start_service().await?;
    Ok(ExitCode::SUCCESS)
}

pub async fn start_service() -> eyre::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                format!("{}=debug,tower_http=debug", env!("CARGO_CRATE_NAME")).into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let router = axum::Router::new()
        .route("/", axum::routing::get(return_hello))
        .route("/attest", axum::routing::get(get_attestation_route))
        .route("/ws", any(ws_handler));
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 8000));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("Listening on {}", addr);
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

pub async fn return_hello() -> &'static str {
    println!("Returning Hello, World!");
    "Hello, World!"
}

pub async fn get_attestation_route() -> Vec<u8> {
    println!("Getting attestation document...");
    // return format!("Attestation document: {:?}", get_attestation_dc());
    // get_attestation().unwrap()
    // let body = reqwest::body::Body::from("Hello, World!")
    //     .send()
    //     .await
    //     .unwrap();
    //
    todo!()
}

fn get_attestation(nonce: Bytes) -> eyre::Result<Vec<u8>> {
    todo!()
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    user_agent: Option<TypedHeader<headers::UserAgent>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    let user_agent = if let Some(TypedHeader(user_agent)) = user_agent {
        user_agent.to_string()
    } else {
        String::from("Unknown browser")
    };
    println!("`{user_agent}` at {addr} connected.");
    // finalize the upgrade process by returning upgrade callback.
    // we can customize the callback by sending additional info such as address.
    ws.on_upgrade(move |socket| handle_socket(socket, addr))
}

/// Actual websocket statemachine (one will be spawned per connection)
async fn handle_socket(mut socket: WebSocket, who: SocketAddr) {
    // send a ping (unsupported by some browsers) just to kick things off and get a response
    // if socket
    //     .send(Message::Ping(Bytes::from_static(&[1, 2, 3])))
    //     .await
    //     .is_ok()
    // {
    //     println!("Pinged {who}...");
    // } else {
    //     println!("Could not send ping {who}!");
    //     // no Error here since the only thing we can do is to close the connection.
    //     // If we can not send messages, there is no way to salvage the statemachine anyway.
    //     return;
    // }

    // receive single message from a client (we can either receive or send with socket).
    // this will likely be the Pong for our Ping or a hello message from client.
    // waiting for message from a client will block this task, but will not block other client's
    // connections.

    //attestation requeste
    if let Some(msg) = socket.recv().await {
        if let Ok(msg) = msg {
            match msg {
                Message::Binary(d) => {
                    println!(">>> {who} sent {} bytes: {d:?}", d.len());
                    if d.len() == 2 {
                        let attestation_doc = get_attestation(d).unwrap();
                        println!("Got attestation document, sending back to {who}...");
                        socket
                            .send(Message::Binary(attestation_doc.into()))
                            .await
                            .unwrap();
                    }
                }
                _ => {
                    println!(
                        "client {who} sent unexpected message, expected binary with attestation request"
                    );
                    return;
                }
            }
        } else {
            println!("client {who} abruptly disconnected or didnt ask attestation");
            return;
        }
    }

    // returning from the handler closes the websocket connection
    println!("Websocket context {who} destroyed");
}

/// helper to print contents of messages to stdout. Has special treatment for Close.
async fn process_message(msg: Message, who: SocketAddr) -> ControlFlow<(), ()> {
    match msg {
        Message::Text(t) => {
            println!(">>> {who} sent str: {t:?}");
        }
        Message::Binary(d) => {
            println!(">>> {who} sent {} bytes: {d:?}", d.len());
            if d.len() == 4 {
                let value = u32::from_be_bytes(d[..4].try_into().unwrap());
                println!("Received u32: {value}");
            }
        }
        Message::Close(c) => {
            if let Some(cf) = c {
                println!(
                    ">>> {who} sent close with code {} and reason `{}`",
                    cf.code, cf.reason
                );
            } else {
                println!(">>> {who} somehow sent close message without CloseFrame");
            }
            return ControlFlow::Break(());
        }

        Message::Pong(v) => {
            println!(">>> {who} sent pong with {v:?}");
        }
        // You should never need to manually handle Message::Ping, as axum's websocket library
        // will do so for you automagically by replying with Pong and copying the v according to
        // spec. But if you need the contents of the pings you can see them here.
        Message::Ping(v) => {
            println!(">>> {who} sent ping with {v:?}");
        }
    }
    ControlFlow::Continue(())
}
