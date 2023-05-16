use anyhow::Result;
use env_logger::Env;
use futures_util::SinkExt;
use log::info;
use tokio::net::{TcpListener, TcpStream};
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::watch::Receiver;

#[tokio::main]
async fn main() -> Result<()> {
    main_async().await
}

async fn main_async() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let (sender, receiver) = tokio::sync::watch::channel(());
    let mut signal_stream = signal(SignalKind::user_defined1())?;
    let pid = std::process::id();
    tokio::spawn(async move {
        loop {
            signal_stream.recv().await;
            println!("Signaling reloads");
            sender.send(()).unwrap();
        }
    });

    let host = std::env::args()
        .nth(2)
        // .unwrap_or_else(|| format!("0.0.0.0:{websocket_port}"));
        .unwrap_or_else(|| String::from("0.0.0.0"));

    let websocket_port = std::env::args()
        .nth(3)
        // .unwrap_or_else(|| format!("0.0.0.0:{websocket_port}"));
        .unwrap_or_else(|| String::from("9001"));

    let address = format!("{host}:{websocket_port}");

    tokio::process::Command::new("cargo")
        .args([
            "watch",
            "--shell",
            "wasm-pack build --target web",
            "--shell",
            &format!("kill -USR1 {pid}"),
        ])
        .env("WATCHRELOAD_PORT", websocket_port)
        .spawn()?;
    println!("Cargo watch is running");

    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&address).await;
    let listener = try_socket.expect("Failed to bind");
    info!("Listening on: {}", address);

    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(accept_connection(stream, receiver.clone()));
    }

    Ok(())
}

async fn accept_connection(stream: TcpStream, mut receiver: Receiver<()>) -> Result<()> {
    let addr = stream
        .peer_addr()
        .expect("connected streams should have a peer address");
    info!("Peer address: {}", addr);

    let mut ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .expect("Error during the websocket handshake occurred");

    info!("New WebSocket connection: {}", addr);

    // only reload on changes that happen after this point
    // TODO: this isn't quite sound -- think through something better
    receiver.borrow_and_update();

    while receiver.changed().await.is_ok() {
        println!("sending reload");
        ws_stream.send("reload".into()).await?;
    }

    Ok(())
}
