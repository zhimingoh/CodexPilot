use once_cell::sync::Lazy;
use std::time::Duration;

static SHARED_HTTP_CLIENT: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .pool_idle_timeout(Duration::from_secs(60))
        .tcp_keepalive(Duration::from_secs(30))
        .user_agent(format!("CodexPilot/{}", crate::version::VERSION))
        .build()
        .expect("failed to build shared HTTP client")
});

pub fn shared() -> &'static reqwest::Client {
    &SHARED_HTTP_CLIENT
}
