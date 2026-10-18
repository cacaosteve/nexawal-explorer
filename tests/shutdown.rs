#![cfg(unix)]

use std::process::{Child, Command, Stdio};
use std::time::Duration;

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test]
async fn sigterm_exits_cleanly_without_force_kill() {
    let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = reservation.local_addr().unwrap();
    drop(reservation);
    let mut process = Process(
        Command::new(env!("CARGO_BIN_EXE_nexawal-explorer"))
            .env("EXPLORER_BIND", addr.to_string())
            .env("EXPLORER_RPC_URL", "http://127.0.0.1:9")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_millis(200))
        .build()
        .unwrap();
    let mut ready = false;
    for _ in 0..100 {
        if let Ok(response) = client.get(format!("http://{addr}/healthz")).send().await {
            if response.status().is_success() {
                ready = true;
                break;
            }
        }
        if process.0.try_wait().unwrap().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(ready, "test child must be serving before receiving SIGTERM");
    assert!(Command::new("kill")
        .args(["-TERM", &process.0.id().to_string()])
        .status()
        .unwrap()
        .success());
    for _ in 0..100 {
        if let Some(status) = process.0.try_wait().unwrap() {
            assert!(
                status.success(),
                "SIGTERM should use graceful shutdown: {status}"
            );
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("server did not stop gracefully within two seconds");
}
