use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct CalculateOrderTemplate {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub saved_at: String,
    #[serde(default)]
    pub order_number: String,
    #[serde(default)]
    pub customer: String,
    #[serde(default)]
    pub product: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub material_display: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub width_mm: f64,
    #[serde(default = "default_waste_percent")]
    pub waste_percent: f64,
    #[serde(default)]
    pub roll_count: Option<f64>,
    #[serde(default)]
    pub first_layer_material: String,
    #[serde(default)]
    pub first_layer_micron: String,
    #[serde(default)]
    pub second_layer_material: String,
    #[serde(default)]
    pub second_layer_micron: String,
    #[serde(default)]
    pub third_layer_material: String,
    #[serde(default)]
    pub third_layer_micron: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Error)]
pub enum CalculateOrderError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("store failed")]
    StoreFailed,
}

#[async_trait]
pub trait CalculateOrderStorePort: Send + Sync {
    async fn list(
        &self,
        owner_key: &str,
    ) -> Result<Vec<CalculateOrderTemplate>, CalculateOrderError>;
    async fn upsert(
        &self,
        owner_key: &str,
        template: CalculateOrderTemplate,
    ) -> Result<CalculateOrderTemplate, CalculateOrderError>;
    async fn delete(&self, owner_key: &str, id: &str) -> Result<(), CalculateOrderError>;
}

pub fn validate_template(template: &CalculateOrderTemplate) -> Result<(), CalculateOrderError> {
    if template.name.trim().is_empty() {
        return Err(CalculateOrderError::InvalidInput(
            "zakaz nomi kerak".to_string(),
        ));
    }
    if template.product.trim().is_empty() {
        return Err(CalculateOrderError::InvalidInput(
            "mahsulot kerak".to_string(),
        ));
    }
    if template.width_mm <= 0.0 {
        return Err(CalculateOrderError::InvalidInput(
            "razmer noto'g'ri".to_string(),
        ));
    }
    if template.waste_percent < 0.0 {
        return Err(CalculateOrderError::InvalidInput(
            "atxod foiz noto'g'ri".to_string(),
        ));
    }
    if template.first_layer_material.trim().is_empty()
        || template.first_layer_micron.trim().is_empty()
    {
        return Err(CalculateOrderError::InvalidInput(
            "1-qavat kerak".to_string(),
        ));
    }
    if template.second_layer_material.trim().is_empty()
        || template.second_layer_micron.trim().is_empty()
    {
        return Err(CalculateOrderError::InvalidInput(
            "2-qavat kerak".to_string(),
        ));
    }
    Ok(())
}

pub fn owner_key(role: &str, ref_: &str) -> String {
    format!("{}:{}", role.trim(), ref_.trim())
}

pub(crate) fn upsert_template(
    store: &mut BTreeMap<String, Vec<CalculateOrderTemplate>>,
    owner_key: &str,
    mut template: CalculateOrderTemplate,
) -> Result<CalculateOrderTemplate, CalculateOrderError> {
    validate_template(&template)?;
    let list = store.entry(owner_key.to_string()).or_default();
    let normalized_name = normalize_name(&template.name);
    let index = list
        .iter()
        .position(|item| normalize_name(&item.name) == normalized_name);
    if let Some(index) = index {
        if template.id.trim().is_empty() {
            template.id = list[index].id.clone();
        }
        list[index] = stamp(template);
        list.sort_by(|left, right| right.saved_at.cmp(&left.saved_at));
        Ok(list[index].clone())
    } else {
        if template.id.trim().is_empty() {
            template.id = new_id();
        }
        let saved = stamp(template);
        list.push(saved.clone());
        list.sort_by(|left, right| right.saved_at.cmp(&left.saved_at));
        Ok(saved)
    }
}

fn stamp(mut template: CalculateOrderTemplate) -> CalculateOrderTemplate {
    template.name = template.name.trim().to_string();
    template.order_number = template.order_number.trim().to_string();
    template.customer = template.customer.trim().to_string();
    template.product = template.product.trim().to_string();
    template.status = template.status.trim().to_string();
    template.material_display = template.material_display.trim().to_string();
    template.color = template.color.trim().to_string();
    template.first_layer_material = template.first_layer_material.trim().to_string();
    template.first_layer_micron = template.first_layer_micron.trim().to_string();
    template.second_layer_material = template.second_layer_material.trim().to_string();
    template.second_layer_micron = template.second_layer_micron.trim().to_string();
    template.third_layer_material = template.third_layer_material.trim().to_string();
    template.third_layer_micron = template.third_layer_micron.trim().to_string();
    template.note = template.note.trim().to_string();
    template.saved_at = unix_micros().to_string();
    template
}

fn normalize_name(value: &str) -> String {
    value.trim().to_lowercase()
}

fn new_id() -> String {
    unix_micros().to_string()
}

fn unix_micros() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_micros())
        .unwrap_or_default()
}

fn default_waste_percent() -> f64 {
    5.0
}
