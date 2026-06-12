use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;

use crate::core::calculate_orders::CalculateOrderTemplate;
use crate::core::formula::{CalculateRequest, LayerInput, calculate};
use crate::core::production_map::{ProductionMapDefinition, ProductionMapNodeKind};

const SHEETS_SCOPE: &str = "https://www.googleapis.com/auth/spreadsheets";
const DEFAULT_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";

#[async_trait]
pub trait OrderSheetSink: Send + Sync {
    async fn append_order(
        &self,
        map: &ProductionMapDefinition,
        template: &CalculateOrderTemplate,
    ) -> Result<(), OrderSheetError>;
}

#[derive(Debug, Clone, Copy)]
pub struct NoopOrderSheetSink;

#[async_trait]
impl OrderSheetSink for NoopOrderSheetSink {
    async fn append_order(
        &self,
        _map: &ProductionMapDefinition,
        _template: &CalculateOrderTemplate,
    ) -> Result<(), OrderSheetError> {
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OrderSheetError {
    #[error("order sheet row is not available")]
    NoRow,
    #[error("google sheets auth failed")]
    AuthFailed,
    #[error("google sheets append failed")]
    AppendFailed,
}

pub fn discover_order_sheet_sink() -> Arc<dyn OrderSheetSink> {
    #[cfg(test)]
    {
        return Arc::new(NoopOrderSheetSink);
    }

    #[allow(unreachable_code)]
    {
        let spreadsheet_id = std::env::var("GOOGLE_SHEETS_ORDER_SPREADSHEET_ID")
            .unwrap_or_default()
            .trim()
            .to_string();
        if spreadsheet_id.is_empty() {
            tracing::info!("order sheets disabled: GOOGLE_SHEETS_ORDER_SPREADSHEET_ID missing");
            return Arc::new(NoopOrderSheetSink);
        }
        let Some(path) = discover_service_account_path() else {
            tracing::warn!("order sheets disabled: service account json missing");
            return Arc::new(NoopOrderSheetSink);
        };
        let raw = match std::fs::read(&path) {
            Ok(raw) => raw,
            Err(error) => {
                tracing::warn!(%error, "order sheets disabled: read service account failed");
                return Arc::new(NoopOrderSheetSink);
            }
        };
        let account: ServiceAccount = match serde_json::from_slice(&raw) {
            Ok(account) => account,
            Err(error) => {
                tracing::warn!(%error, "order sheets disabled: parse service account failed");
                return Arc::new(NoopOrderSheetSink);
            }
        };
        let range = std::env::var("GOOGLE_SHEETS_ORDER_RANGE")
            .unwrap_or_else(|_| "A:P".to_string())
            .trim()
            .to_string();
        Arc::new(GoogleSheetsOrderSink::new(account, spreadsheet_id, range))
    }
}

fn discover_service_account_path() -> Option<std::path::PathBuf> {
    for key in [
        "GOOGLE_SHEETS_SERVICE_ACCOUNT_PATH",
        "GOOGLE_SERVICE_ACCOUNT_PATH",
        "FCM_SERVICE_ACCOUNT_PATH",
    ] {
        if let Ok(env) = std::env::var(key) {
            let path = std::path::PathBuf::from(env.trim());
            if !path.as_os_str().is_empty() && path.is_file() {
                return Some(path);
            }
        }
    }
    let fallback = std::path::PathBuf::from("service-account.json");
    fallback.is_file().then_some(fallback)
}

struct GoogleSheetsOrderSink {
    http_client: reqwest::Client,
    token_provider: ServiceAccountTokenProvider,
    endpoint: String,
}

impl GoogleSheetsOrderSink {
    fn new(account: ServiceAccount, spreadsheet_id: String, range: String) -> Self {
        let encoded_range = urlencoding::encode(range.trim());
        Self {
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .expect("reqwest client"),
            token_provider: ServiceAccountTokenProvider::new(account),
            endpoint: format!(
                "https://sheets.googleapis.com/v4/spreadsheets/{spreadsheet_id}/values/{encoded_range}:append?valueInputOption=USER_ENTERED&insertDataOption=INSERT_ROWS"
            ),
        }
    }
}

#[async_trait]
impl OrderSheetSink for GoogleSheetsOrderSink {
    async fn append_order(
        &self,
        map: &ProductionMapDefinition,
        template: &CalculateOrderTemplate,
    ) -> Result<(), OrderSheetError> {
        let row = order_sheet_row(map, template).ok_or(OrderSheetError::NoRow)?;
        let access_token = self.token_provider.access_token(&self.http_client).await?;
        let payload = AppendValuesRequest { values: vec![row] };
        let response = self
            .http_client
            .post(&self.endpoint)
            .bearer_auth(access_token)
            .json(&payload)
            .send()
            .await
            .map_err(|_| OrderSheetError::AppendFailed)?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(OrderSheetError::AppendFailed)
        }
    }
}

pub fn is_sheet_order_map(map: &ProductionMapDefinition) -> bool {
    let id = map.id.trim();
    let order_number = map.order_number.trim();
    id.starts_with("zakaz-")
        && order_number.len() == 4
        && order_number.chars().all(|ch| ch.is_ascii_digit())
}

fn order_sheet_row(
    map: &ProductionMapDefinition,
    template: &CalculateOrderTemplate,
) -> Option<Vec<Value>> {
    if !is_sheet_order_map(map) || template.kg <= 0.0 {
        return None;
    }
    let calculation = calculate(CalculateRequest {
        order_number: Some(map.order_number.trim().to_string()),
        product: Some(template.product.trim().to_string()),
        kg: Some(template.kg),
        width_mm: Some(template.width_mm),
        waste_percent: Some(template.waste_percent),
        roll_count: template.roll_count,
        first_layer: LayerInput::new(
            template.first_layer_material.trim(),
            template.first_layer_micron.trim(),
        ),
        second_layer: LayerInput::new(
            template.second_layer_material.trim(),
            template.second_layer_micron.trim(),
        ),
        third_layer: LayerInput::new(
            template.third_layer_material.trim(),
            template.third_layer_micron.trim(),
        ),
        ..CalculateRequest::default()
    })
    .ok()?;
    let first_result = calculation.results.first()?;
    let now = time::OffsetDateTime::now_utc()
        .to_offset(time::UtcOffset::from_hms(5, 0, 0).expect("valid tashkent offset"));
    Some(vec![
        Value::String(sheet_press_marker(map, template)),
        Value::String(format!("{:02}/{:02}", now.day(), u8::from(now.month()))),
        Value::String(format!("{:02}:{:02}", now.hour(), now.minute())),
        Value::String(sheet_order_code(map, template)),
        Value::String(first_non_empty([
            &template.product,
            &template.name,
            &map.title,
        ])),
        json_number(template.kg),
        Value::String(template.first_layer_material.trim().to_string()),
        Value::String(template.second_layer_material.trim().to_string()),
        Value::String(template.third_layer_material.trim().to_string()),
        json_number(template.width_mm),
        Value::String(template.first_layer_micron.trim().to_string()),
        Value::String(template.second_layer_micron.trim().to_string()),
        Value::String(template.third_layer_micron.trim().to_string()),
        json_number(first_result.rounded_length),
        template
            .roll_count
            .map(json_number)
            .unwrap_or_else(|| Value::String(String::new())),
        json_number(f64::from(calculation.rubber_size_mm)),
    ])
}

fn sheet_press_marker(map: &ProductionMapDefinition, template: &CalculateOrderTemplate) -> String {
    let product = format!("{} {}", template.product, template.name).to_lowercase();
    if product.contains("flex") || product.contains("fleks") || product.contains("flekso") {
        return "F".to_string();
    }
    let mut titles = Vec::new();
    for node in &map.nodes {
        if node.kind != ProductionMapNodeKind::Apparatus {
            continue;
        }
        let assigned = node.alternative_assigned_title.trim();
        if !assigned.is_empty() {
            titles.insert(0, assigned.to_string());
        }
        titles.push(node.title.trim().to_string());
    }
    for title in titles {
        let lower = title.to_lowercase();
        if lower.contains("flex") || lower.contains("fleks") || lower.contains("flekso") {
            return "F".to_string();
        }
        for marker in ["9", "8", "7"] {
            if lower.contains(&format!("{marker} ta rangli"))
                || lower.contains(&format!("{marker} rangli"))
            {
                return marker.to_string();
            }
        }
    }
    String::new()
}

fn sheet_order_code(map: &ProductionMapDefinition, template: &CalculateOrderTemplate) -> String {
    let code = first_non_empty([
        map.order_number.as_str(),
        map.code.as_str(),
        template.order_number.as_str(),
        template.code.as_str(),
    ]);
    if code.starts_with('/') {
        code
    } else {
        format!("/{code}")
    }
}

fn first_non_empty<const N: usize>(values: [&str; N]) -> String {
    values
        .into_iter()
        .map(str::trim)
        .find(|value| !value.is_empty())
        .unwrap_or("")
        .to_string()
}

fn json_number(value: f64) -> Value {
    serde_json::Number::from_f64(value)
        .map(Value::Number)
        .unwrap_or_else(|| Value::String(String::new()))
}

#[derive(Debug, Clone, Deserialize)]
struct ServiceAccount {
    client_email: String,
    private_key: String,
    #[serde(default)]
    token_uri: String,
}

#[derive(Debug)]
struct ServiceAccountTokenProvider {
    account: ServiceAccount,
    cache: Mutex<Option<CachedAccessToken>>,
}

impl ServiceAccountTokenProvider {
    fn new(account: ServiceAccount) -> Self {
        Self {
            account,
            cache: Mutex::new(None),
        }
    }

    async fn access_token(&self, client: &reqwest::Client) -> Result<String, OrderSheetError> {
        let mut cache = self.cache.lock().await;
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        if let Some(cached) = cache.as_ref()
            && cached.expires_at > now + 60
        {
            return Ok(cached.access_token.clone());
        }

        let token_uri = self.token_uri();
        let assertion = self.signed_assertion(now, &token_uri)?;
        let form = format!(
            "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer&assertion={}",
            urlencoding::encode(&assertion)
        );
        let response = client
            .post(&token_uri)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(form)
            .send()
            .await
            .map_err(|_| OrderSheetError::AuthFailed)?;
        if !response.status().is_success() {
            return Err(OrderSheetError::AuthFailed);
        }
        let token: OAuthTokenResponse = response
            .json()
            .await
            .map_err(|_| OrderSheetError::AuthFailed)?;
        let expires_at = now + token.expires_in.unwrap_or(3600);
        *cache = Some(CachedAccessToken {
            access_token: token.access_token.clone(),
            expires_at,
        });
        Ok(token.access_token)
    }

    fn token_uri(&self) -> String {
        let value = self.account.token_uri.trim();
        if value.is_empty() {
            DEFAULT_TOKEN_URI.to_string()
        } else {
            value.to_string()
        }
    }

    fn signed_assertion(&self, now: i64, token_uri: &str) -> Result<String, OrderSheetError> {
        let claims = JwtClaims {
            iss: self.account.client_email.trim(),
            scope: SHEETS_SCOPE,
            aud: token_uri,
            iat: now,
            exp: now + 3600,
        };
        let key = EncodingKey::from_rsa_pem(self.account.private_key.as_bytes())
            .map_err(|_| OrderSheetError::AuthFailed)?;
        encode(&Header::new(Algorithm::RS256), &claims, &key)
            .map_err(|_| OrderSheetError::AuthFailed)
    }
}

#[derive(Debug, Clone)]
struct CachedAccessToken {
    access_token: String,
    expires_at: i64,
}

#[derive(Debug, Deserialize)]
struct OAuthTokenResponse {
    access_token: String,
    expires_in: Option<i64>,
}

#[derive(Serialize)]
struct JwtClaims<'a> {
    iss: &'a str,
    scope: &'a str,
    aud: &'a str,
    iat: i64,
    exp: i64,
}

#[derive(Serialize)]
struct AppendValuesRequest {
    values: Vec<Vec<Value>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::production_map::{
        ProductionMapEdge, ProductionMapNode, ProductionMapNodeKind,
    };

    #[test]
    fn order_sheet_row_matches_legacy_excel_columns() {
        let map = ProductionMapDefinition {
            id: "zakaz-7775".to_string(),
            product_code: "ITEM-1".to_string(),
            title: "fibre Mahsulot: XXL pack 70 sht".to_string(),
            code: "7775".to_string(),
            order_number: "7775".to_string(),
            roll_count: Some(8.0),
            width_mm: Some(735.0),
            nodes: vec![ProductionMapNode {
                id: "apparatus".to_string(),
                kind: ProductionMapNodeKind::Apparatus,
                title: "7 ta rangli pechat".to_string(),
                formula: None,
                role_code: String::new(),
                item_code: String::new(),
                qty_formula: String::new(),
                from_location: String::new(),
                to_location: String::new(),
                alternative_group_id: String::new(),
                alternative_group_label: String::new(),
                alternative_assigned_title: "8 ta rangli pechat".to_string(),
                x: 0.0,
                y: 0.0,
            }],
            edges: Vec::<ProductionMapEdge>::new(),
        };
        let template = CalculateOrderTemplate {
            name: "fibre Mahsulot: XXL pack 70 sht".to_string(),
            product: "fibre Mahsulot: XXL pack 70 sht".to_string(),
            width_mm: 735.0,
            waste_percent: 5.0,
            roll_count: Some(8.0),
            first_layer_material: "pet".to_string(),
            first_layer_micron: "12".to_string(),
            second_layer_material: "pe oq".to_string(),
            second_layer_micron: "55".to_string(),
            kg: 600.0,
            ..CalculateOrderTemplate::default()
        };

        let row = order_sheet_row(&map, &template).expect("row");

        assert_eq!(row.len(), 16);
        assert_eq!(row[0], Value::String("8".to_string()));
        assert_eq!(row[3], Value::String("/7775".to_string()));
        assert_eq!(
            row[4],
            Value::String("fibre Mahsulot: XXL pack 70 sht".to_string())
        );
        assert_eq!(row[6], Value::String("pet".to_string()));
        assert_eq!(row[7], Value::String("pe oq".to_string()));
        assert_eq!(row[9], json_number(735.0));
        assert_eq!(row[10], Value::String("12".to_string()));
        assert_eq!(row[11], Value::String("55".to_string()));
        assert_eq!(row[14], json_number(8.0));
        assert_eq!(row[15], json_number(750.0));
    }

    #[test]
    fn order_sheet_row_marks_flexo_orders_with_f() {
        let map = ProductionMapDefinition {
            id: "zakaz-1123".to_string(),
            product_code: "ITEM-F".to_string(),
            title: "fleksa lec Mahsulot".to_string(),
            code: String::new(),
            order_number: "1123".to_string(),
            roll_count: None,
            width_mm: Some(1190.0),
            nodes: Vec::new(),
            edges: Vec::new(),
        };
        let template = CalculateOrderTemplate {
            name: "fleksa lec Mahsulot".to_string(),
            product: "fleksa lec Mahsulot".to_string(),
            width_mm: 1190.0,
            waste_percent: 5.0,
            first_layer_material: "pe pr".to_string(),
            first_layer_micron: "50".to_string(),
            second_layer_material: "pe pr".to_string(),
            second_layer_micron: "30".to_string(),
            kg: 500.0,
            ..CalculateOrderTemplate::default()
        };

        let row = order_sheet_row(&map, &template).expect("row");

        assert_eq!(row[0], Value::String("F".to_string()));
        assert_eq!(row[3], Value::String("/1123".to_string()));
    }
}
