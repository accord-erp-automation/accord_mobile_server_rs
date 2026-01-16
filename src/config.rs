use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use crate::error::AppError;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub bind_addr: SocketAddr,
    pub session_store_path: PathBuf,
    pub session_ttl_seconds: Option<u64>,
    pub supplier_prefix: String,
    pub werka_prefix: String,
    pub werka_code: String,
    pub werka_name: String,
    pub admin_phone: String,
    pub admin_name: String,
    pub admin_code: String,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, AppError> {
        let addr = std::env::var("MOBILE_API_ADDR").unwrap_or_else(|_| ":8081".to_string());
        let session_path = std::env::var("MOBILE_API_SESSION_STORE_PATH")
            .or_else(|_| std::env::var("MOBILE_API_SESSION_STORE"))
            .unwrap_or_else(|_| "data/mobile_sessions.json".to_string());
        let ttl_hours = std::env::var("MOBILE_API_SESSION_TTL_HOURS")
            .ok()
            .and_then(|raw| raw.trim().parse::<u64>().ok())
            .unwrap_or(24 * 30);

        Ok(Self {
            bind_addr: parse_bind_addr(&addr)?,
            session_store_path: PathBuf::from(session_path),
            session_ttl_seconds: Some(Duration::from_secs(ttl_hours * 60 * 60).as_secs()),
            supplier_prefix: env_or("MOBILE_DEV_SUPPLIER_PREFIX", "10"),
            werka_prefix: env_or("MOBILE_DEV_WERKA_PREFIX", "20"),
            werka_code: env_or("MOBILE_DEV_WERKA_CODE", ""),
            werka_name: env_or("MOBILE_DEV_WERKA_NAME", "Werka"),
            admin_phone: "+998880000000".to_string(),
            admin_name: "Admin".to_string(),
            admin_code: "19621978".to_string(),
        })
    }
}

fn env_or(key: &str, fallback: &str) -> String {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn parse_bind_addr(raw: &str) -> Result<SocketAddr, AppError> {
    let trimmed = raw.trim();
    let normalized = if trimmed.starts_with(':') {
        format!("0.0.0.0{trimmed}")
    } else {
        trimmed.to_string()
    };

    normalized.parse().map_err(|_| AppError::InvalidConfig {
        key: "MOBILE_API_ADDR",
        value: raw.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::parse_bind_addr;

    #[test]
    fn parses_go_style_bind_addr() {
        let addr = parse_bind_addr(":8081").expect("addr");

        assert_eq!(addr.to_string(), "0.0.0.0:8081");
    }
}
