use super::*;

impl ErpnextClient {
    pub(super) async fn admin_get_json<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T, AdminPortError> {
        let response = self
            .http
            .get(format!("{}{}", self.base_url(), path))
            .header(reqwest::header::AUTHORIZATION, self.auth_header())
            .query(query)
            .send()
            .await
            .map_err(|_| AdminPortError::LookupFailed)?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(AdminPortError::NotFound);
        }
        let body = response
            .text()
            .await
            .map_err(|_| AdminPortError::LookupFailed)?;
        if !status.is_success() {
            return Err(AdminPortError::LookupFailed);
        }
        serde_json::from_str(&body).map_err(|_| AdminPortError::LookupFailed)
    }

    pub(super) async fn admin_json_request<T: for<'de> Deserialize<'de>>(
        &self,
        method: reqwest::Method,
        path: &str,
        payload: serde_json::Value,
    ) -> Result<T, AdminPortError> {
        let response = self
            .http
            .request(method, format!("{}{}", self.base_url(), path))
            .header(reqwest::header::AUTHORIZATION, self.auth_header())
            .json(&payload)
            .send()
            .await
            .map_err(|_| AdminPortError::LookupFailed)?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(AdminPortError::NotFound);
        }
        let body = response
            .text()
            .await
            .map_err(|_| AdminPortError::LookupFailed)?;
        if !status.is_success() {
            return Err(AdminPortError::LookupFailed);
        }
        serde_json::from_str(&body).map_err(|_| AdminPortError::LookupFailed)
    }

    pub(super) async fn admin_empty_request(
        &self,
        method: reqwest::Method,
        path: &str,
        payload: serde_json::Value,
    ) -> Result<(), AdminPortError> {
        let mut request = self
            .http
            .request(method, format!("{}{}", self.base_url(), path))
            .header(reqwest::header::AUTHORIZATION, self.auth_header());
        if !payload.is_null() {
            request = request.json(&payload);
        }
        let response = request
            .send()
            .await
            .map_err(|_| AdminPortError::LookupFailed)?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(AdminPortError::NotFound);
        }
        response
            .error_for_status()
            .map(|_| ())
            .map_err(|_| AdminPortError::LookupFailed)
    }
}
