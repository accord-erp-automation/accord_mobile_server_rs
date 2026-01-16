use crate::config::AppConfig;
use crate::core::auth::models::{Principal, PrincipalRole};

#[derive(Debug, Clone)]
pub struct AuthService {
    supplier_prefix: String,
    werka_prefix: String,
    werka_code: String,
    werka_name: String,
    admin_phone: String,
    admin_name: String,
    admin_code: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AuthError {
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("invalid role")]
    InvalidRole,
}

impl AuthService {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            supplier_prefix: blank_default(&config.supplier_prefix, "10"),
            werka_prefix: blank_default(&config.werka_prefix, "20"),
            werka_code: config.werka_code.trim().to_string(),
            werka_name: blank_default(&config.werka_name, "Werka"),
            admin_phone: normalize_config_phone(&config.admin_phone)
                .unwrap_or_else(|_| config.admin_phone.trim().to_string()),
            admin_name: blank_default(&config.admin_name, "Admin"),
            admin_code: config.admin_code.trim().to_string(),
        }
    }

    pub async fn login(&self, phone: &str, code: &str) -> Result<Principal, AuthError> {
        let normalized_phone = normalize_phone(phone).map_err(|_| AuthError::InvalidCredentials)?;
        let code = code.trim();

        if !self.admin_phone.is_empty()
            && self.admin_phone.eq_ignore_ascii_case(&normalized_phone)
            && !self.admin_code.is_empty()
            && code == self.admin_code
        {
            return Ok(Principal {
                role: PrincipalRole::Admin,
                display_name: self.admin_name.clone(),
                legal_name: self.admin_name.clone(),
                ref_: "admin".to_string(),
                phone: normalized_phone,
                avatar_url: String::new(),
            });
        }

        match self.infer_role(code)? {
            PrincipalRole::Werka => self.login_werka(normalized_phone, code),
            PrincipalRole::Supplier | PrincipalRole::Customer => Err(AuthError::InvalidCredentials),
            PrincipalRole::Admin => Err(AuthError::InvalidRole),
        }
    }

    fn login_werka(&self, normalized_phone: String, code: &str) -> Result<Principal, AuthError> {
        if !code.is_empty() && code == self.werka_code {
            return Ok(Principal {
                role: PrincipalRole::Werka,
                display_name: self.werka_name.clone(),
                legal_name: self.werka_name.clone(),
                ref_: "werka".to_string(),
                phone: normalized_phone,
                avatar_url: String::new(),
            });
        }

        Err(AuthError::InvalidCredentials)
    }

    fn infer_role(&self, code: &str) -> Result<PrincipalRole, AuthError> {
        let trimmed = code.trim();

        if trimmed.starts_with(&self.supplier_prefix) {
            Ok(PrincipalRole::Supplier)
        } else if trimmed.starts_with(&self.werka_prefix) {
            Ok(PrincipalRole::Werka)
        } else if trimmed.starts_with("30") {
            Ok(PrincipalRole::Customer)
        } else {
            Err(AuthError::InvalidRole)
        }
    }
}

pub fn normalize_phone(input: &str) -> Result<String, AuthError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(AuthError::InvalidCredentials);
    }

    let mut digits = String::new();
    for ch in trimmed.chars() {
        if ch == '+' {
            continue;
        }
        if !ch.is_ascii_digit() {
            return Err(AuthError::InvalidCredentials);
        }
        digits.push(ch);
    }

    if digits.len() < 9 || digits.len() > 12 {
        return Err(AuthError::InvalidCredentials);
    }

    Ok(format!("+{digits}"))
}

fn normalize_config_phone(phone: &str) -> Result<String, AuthError> {
    let mut clean = phone
        .replace(' ', "")
        .replace('-', "")
        .replace('(', "")
        .replace(')', "");

    if !clean.trim().starts_with('+') && clean.len() == 9 {
        clean = format!("998{clean}");
    }

    normalize_phone(&clean)
}

fn blank_default(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{AuthService, normalize_phone};
    use crate::config::AppConfig;
    use crate::core::auth::models::PrincipalRole;

    fn config() -> AppConfig {
        AppConfig {
            bind_addr: "127.0.0.1:8081".parse().expect("addr"),
            session_store_path: "data/mobile_sessions.json".into(),
            session_ttl_seconds: Some(30 * 24 * 60 * 60),
            supplier_prefix: "10".to_string(),
            werka_prefix: "20".to_string(),
            werka_code: "20ABCDEF1234".to_string(),
            werka_name: "Werka".to_string(),
            admin_phone: "+998880000000".to_string(),
            admin_name: "Admin".to_string(),
            admin_code: "19621978".to_string(),
        }
    }

    #[test]
    fn normalizes_phone_like_go() {
        assert_eq!(normalize_phone("998901234567").unwrap(), "+998901234567");
        assert!(normalize_phone("+12345").is_err());
        assert!(normalize_phone("+998 90").is_err());
    }

    #[tokio::test]
    async fn admin_login_does_not_need_erp() {
        let auth = AuthService::new(&config());
        let principal = auth
            .login("+998880000000", "19621978")
            .await
            .expect("admin login");

        assert_eq!(principal.role, PrincipalRole::Admin);
        assert_eq!(principal.ref_, "admin");
    }

    #[tokio::test]
    async fn werka_login_is_code_driven() {
        let auth = AuthService::new(&config());
        let principal = auth
            .login("+123456789", "20ABCDEF1234")
            .await
            .expect("werka login");

        assert_eq!(principal.role, PrincipalRole::Werka);
        assert_eq!(principal.ref_, "werka");
    }
}
