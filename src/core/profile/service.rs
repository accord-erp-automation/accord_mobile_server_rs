use std::sync::Arc;

use crate::core::auth::models::{Principal, PrincipalRole};
use crate::core::profile::ports::{DownloadedFile, ProfileLookup, ProfilePortError};

#[derive(Clone)]
pub struct ProfileService {
    erp_base_url: String,
    lookup: Option<Arc<dyn ProfileLookup>>,
}

impl ProfileService {
    pub fn new(erp_base_url: String) -> Self {
        Self {
            erp_base_url: erp_base_url.trim().trim_end_matches('/').to_string(),
            lookup: None,
        }
    }

    pub fn with_erp_lookup(mut self, lookup: Arc<dyn ProfileLookup>) -> Self {
        self.lookup = Some(lookup);
        self
    }

    pub async fn refresh(&self, mut principal: Principal) -> Principal {
        let Some(lookup) = &self.lookup else {
            return principal;
        };

        match principal.role {
            PrincipalRole::Supplier => {
                if let Ok(profile) = lookup.get_supplier_profile(&principal.ref_).await {
                    principal.phone = profile.phone;
                    if !profile.image.trim().is_empty() {
                        principal.avatar_url =
                            absolute_file_url(&self.erp_base_url, &profile.image);
                    }
                }
            }
            PrincipalRole::Customer => {
                if let Ok(profile) = lookup.get_customer_profile(&principal.ref_).await {
                    principal.phone = profile.phone;
                }
            }
            PrincipalRole::Werka | PrincipalRole::Admin => {}
        }

        if principal.display_name.is_empty() {
            principal.display_name = principal.legal_name.clone();
        }

        principal
    }

    pub async fn download_avatar(
        &self,
        principal: Principal,
    ) -> Result<Option<DownloadedFile>, ProfilePortError> {
        if principal.role != PrincipalRole::Supplier {
            return Ok(None);
        }

        let current = self.refresh(principal).await;
        if current.avatar_url.trim().is_empty() {
            return Ok(None);
        }

        let Some(lookup) = &self.lookup else {
            return Err(ProfilePortError::LookupFailed);
        };

        lookup.download_file(&current.avatar_url).await.map(Some)
    }
}

fn absolute_file_url(base_url: &str, file_url: &str) -> String {
    let trimmed = file_url.trim();
    if trimmed.is_empty() || trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("{}{}", base_url.trim_end_matches('/'), trimmed)
    }
}
