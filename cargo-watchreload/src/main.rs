use anyhow::Result;
use clap::Parser;
use env_logger::Env;
use futures_util::SinkExt;
use log::info;
use tokio::net::{TcpListener, TcpStream};
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::watch::Receiver;

#[derive(Debug, Parser)]
/// Sets up a server for the web app to connect to via a websocket, then uses `cargo watch` to
/// recompile the app when it changes and tells the app to reload itself via the websocket after
/// the compilation is done.
///
/// Compiles the app via `wasm-pack`, which must already be installed and available on the `PATH`.
struct CliArgs {
    #[clap(long, default_value = "0.0.0.0")]
    host: String,
    #[clap(long, default_value = "9001")]
    websocket_port: String,
    #[clap(
        trailing_var_arg = true,
        help = "Forwarded to the `wasm-pack` invocation"
    )]
    wasm_pack_args: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    main_async().await
}

async fn main_async() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    // When invoked as `cargo watchreload` cargo will add `watchreload` as the first argument,
    // which messes up Clap, so we strip it out.
    let args = if std::env::args().nth(1) == Some("watchreload".into()) {
        // Skip the 1th element
        CliArgs::parse_from(
            std::env::args()
                .enumerate()
                .filter(|(i, _)| *i != 1)
                .map(|(_, a)| a),
        )
    } else {
        CliArgs::parse()
    };

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

    let address = format!("{}:{}", args.host, args.websocket_port);

    let mut child = tokio::process::Command::new("cargo")
        .args([
            "watch",
            "--shell",
            // TODO - Shell escape the args?
            &format!(
                "wasm-pack build --target web {}",
                args.wasm_pack_args.join(" ")
            ),
            "--shell",
            &format!("kill -USR1 {pid}"),
        ])
        .env("WATCHRELOAD_PORT", args.websocket_port)
        .spawn()?;
    println!("Cargo watch is running");

    // Create the event loop and TCP listener we'll accept connections on.
    let listener = match TcpListener::bind(&address).await {
        Ok(listener) => listener,
        Err(e) => {
            child.kill().await?;
            panic!("Failed to bind: {e}");
        }
    };
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
