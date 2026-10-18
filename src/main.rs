use nexawal_explorer::{model::DEFAULT_MAX_TIP_AGE_SECS, rpc::Rpc, web};
use std::{env, net::SocketAddr};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bind: SocketAddr = env::var("EXPLORER_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8080".into())
        .parse()?;
    let upstream =
        env::var("EXPLORER_RPC_URL").unwrap_or_else(|_| "https://rpc.nexawal.com".into());
    let max_tip_age_secs = match env::var("EXPLORER_MAX_TIP_AGE_SECS") {
        Ok(value) => value.parse::<u64>()?,
        Err(env::VarError::NotPresent) => DEFAULT_MAX_TIP_AGE_SECS,
        Err(error) => return Err(error.into()),
    };
    let rpc = Rpc::with_max_tip_age(&upstream, max_tip_age_secs)?;
    let listener = tokio::net::TcpListener::bind(bind).await?;
    // Never log visitor IPs, URIs, search terms, transaction IDs or upstream credentials.
    eprintln!(
        "NexaWal Explorer {} listening on {bind}; access logging disabled",
        env!("CARGO_PKG_VERSION")
    );
    axum::serve(listener, web::router(rpc))
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}

async fn shutdown() {
    #[cfg(unix)]
    {
        if let Ok(mut terminate) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {},
                _ = terminate.recv() => {},
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}
