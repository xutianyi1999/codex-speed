mod metrics;
mod server;
mod store;
#[cfg(test)]
mod tests;

use anyhow::{Context, Result};
use clap::Parser;
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::PathBuf,
    sync::Arc,
};
use tokio::sync::watch;

#[derive(Parser, Debug)]
#[command(version, about = "Local web monitor for native Codex OTLP metrics")]
struct Args {
    /// IP address to listen on.
    #[arg(long, default_value = "0.0.0.0")]
    host: IpAddr,
    /// Local HTTP port for both the dashboard and /v1/metrics.
    #[arg(long, default_value_t = 4318)]
    port: u16,
    /// Directory containing the metrics database (seven-day retention).
    #[arg(long)]
    data_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let data_dir = args
        .data_dir
        .or_else(|| dirs::data_local_dir().map(|p| p.join("codex-speed")))
        .context("Cannot locate a data directory; pass --data-dir")?;
    std::fs::create_dir_all(&data_dir)?;
    let store = store::Store::open(&data_dir.join("metrics.sqlite")).await?;
    let (updates, _) = watch::channel(Some(0_u64));
    let listener = tokio::net::TcpListener::bind((args.host, args.port)).await?;
    let address = listener.local_addr()?;
    let dashboard = if address.ip().is_unspecified() {
        SocketAddr::new(
            match address.ip() {
                IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::LOCALHOST),
                IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::LOCALHOST),
            },
            address.port(),
        )
    } else {
        address
    };
    let state = server::AppState {
        store: Arc::new(store),
        updates: updates.clone(),
        endpoint: format!("http://{dashboard}/v1/metrics"),
    };
    println!("Listening:   {address}");
    #[cfg(feature = "embedded-web")]
    println!("Codex Speed: http://{dashboard}");
    #[cfg(not(feature = "embedded-web"))]
    println!("API-only development server; dashboard is served by Vite on port 5173");
    println!("Metrics:     {}", state.endpoint);
    println!("Data:        {}", data_dir.display());
    let mut shutdown = updates.subscribe();
    let server = axum::serve(listener, server::router(state)).with_graceful_shutdown(async move {
        let _ = shutdown.wait_for(Option::is_none).await;
    });
    tokio::select! {
        result = server => result?,
        _ = async {
            let _ = tokio::signal::ctrl_c().await;
            // End every SSE stream before waiting for HTTP connections to drain.
            updates.send_replace(None);
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        } => eprintln!("Shutdown deadline reached; closing remaining connections"),
    }
    Ok(())
}
