use async_trait::async_trait;
use reqwest::Method;
use serde::Deserialize;
use serde_json::Value;

use crate::core::calculate_orders::CalculateOrderTemplate;
use crate::core::formula::{CalculateRequest, LayerInput, calculate};
use crate::core::production_map::{
    ProductionMapDefinition, ProductionMapNode, ProductionMapNodeKind,
};
use crate::erpnext::client::ErpnextClient;

#[async_trait]
pub trait ProductionOrderErpSink: Send + Sync {
    async fn save_order(
        &self,
        map: &ProductionMapDefinition,
        template: &CalculateOrderTemplate,
    ) -> Result<(), ProductionOrderErpError>;
}

#[derive(Debug, Clone, Copy)]
pub struct NoopProductionOrderErpSink;

#[async_trait]
impl ProductionOrderErpSink for NoopProductionOrderErpSink {
    async fn save_order(
        &self,
        _map: &ProductionMapDefinition,
        _template: &CalculateOrderTemplate,
    ) -> Result<(), ProductionOrderErpError> {
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProductionOrderErpError {
    #[error("erp production order write failed: {0}")]
    WriteFailed(String),
}

#[async_trait]
impl ProductionOrderErpSink for ErpnextClient {
    async fn save_order(
        &self,
        map: &ProductionMapDefinition,
        template: &CalculateOrderTemplate,
    ) -> Result<(), ProductionOrderErpError> {
        if template.kg <= 0.0 {
            return Ok(());
        }
        let item_code = first_non_empty([&template.item_code, &map.product_code]);
        if item_code.is_empty() {
            return Err(ProductionOrderErpError::WriteFailed(
                "item code is empty".to_string(),
            ));
        }
        let item = self.erp_item(&item_code).await?;
        if item.default_bom.trim().is_empty() {
            return Err(ProductionOrderErpError::WriteFailed(format!(
                "item default_bom is empty: {item_code}"
            )));
        }
        let fg_warehouse = self.resolve_fg_warehouse().await?;
        let company = self.erp_warehouse_company(&fg_warehouse).await?;
        let mut payload = build_work_order_payload(map, template);
        payload["company"] = Value::String(company);
        payload["fg_warehouse"] = Value::String(fg_warehouse);
        payload["bom_no"] = Value::String(item.default_bom);
        payload["stock_uom"] = Value::String(item.stock_uom);
        payload["planned_start_date"] = Value::String(now_erp_datetime());
        let _: ResourceResponse<NameRow> = self
            .production_order_json_request(Method::POST, "/api/resource/Work Order", Some(payload))
            .await?;
        Ok(())
    }
}

impl ErpnextClient {
    async fn erp_item(&self, item_code: &str) -> Result<ItemRow, ProductionOrderErpError> {
        let response: ResourceResponse<ItemRow> = self
            .production_order_json_request(
                Method::GET,
                &format!(
                    "/api/resource/Item/{}",
                    urlencoding::encode(item_code.trim())
                ),
                None,
            )
            .await?;
        Ok(response.data)
    }

    async fn resolve_fg_warehouse(&self) -> Result<String, ProductionOrderErpError> {
        let configured = self.default_warehouse();
        if !configured.trim().is_empty() {
            return Ok(configured.trim().to_string());
        }
        let response: ListResponse<NameRow> = self
            .production_order_get(
                "/api/resource/Warehouse",
                &[
                    ("fields", r#"["name"]"#.to_string()),
                    ("limit_page_length", "1".to_string()),
                ],
            )
            .await?;
        response
            .data
            .into_iter()
            .map(|row| row.name.trim().to_string())
            .find(|name| !name.is_empty())
            .ok_or_else(|| ProductionOrderErpError::WriteFailed("warehouse not found".to_string()))
    }

    async fn erp_warehouse_company(
        &self,
        warehouse: &str,
    ) -> Result<String, ProductionOrderErpError> {
        let response: ResourceResponse<WarehouseRow> = self
            .production_order_json_request(
                Method::GET,
                &format!(
                    "/api/resource/Warehouse/{}",
                    urlencoding::encode(warehouse.trim())
                ),
                None,
            )
            .await?;
        let company = response.data.company.trim().to_string();
        if company.is_empty() {
            Err(ProductionOrderErpError::WriteFailed(format!(
                "warehouse company is empty: {}",
                warehouse.trim()
            )))
        } else {
            Ok(company)
        }
    }

    async fn production_order_get<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T, ProductionOrderErpError> {
        let response = self
            .http
            .get(format!("{}{}", self.base_url(), encoded_path(path)))
            .header(reqwest::header::AUTHORIZATION, self.auth_header().await)
            .query(query)
            .send()
            .await
            .map_err(|error| ProductionOrderErpError::WriteFailed(error.to_string()))?;
        decode_response(response).await
    }

    async fn production_order_json_request<T: for<'de> Deserialize<'de>>(
        &self,
        method: Method,
        path: &str,
        payload: Option<Value>,
    ) -> Result<T, ProductionOrderErpError> {
        let mut request = self
            .http
            .request(method, format!("{}{}", self.base_url(), encoded_path(path)))
            .header(reqwest::header::AUTHORIZATION, self.auth_header().await);
        if let Some(payload) = payload {
            request = request.json(&payload);
        }
        let response = request
            .send()
            .await
            .map_err(|error| ProductionOrderErpError::WriteFailed(error.to_string()))?;
        decode_response(response).await
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn build_work_order_payload(
    map: &ProductionMapDefinition,
    template: &CalculateOrderTemplate,
) -> Value {
    let item_code = first_non_empty([&template.item_code, &map.product_code]);
    let order_number = first_non_empty([
        &map.order_number,
        &map.code,
        &template.order_number,
        &template.code,
    ]);
    let product_name = first_non_empty([&template.product, &template.name, &map.title]);
    let roll_count = template.roll_count.or(map.roll_count).unwrap_or_default();
    let rubber_size = rubber_size_mm(template).unwrap_or_default();
    let operations: Vec<Value> = apparatus_nodes_in_flow_order(map)
        .into_iter()
        .enumerate()
        .map(|(index, node)| {
            serde_json::json!({
                "operation": operation_name(&node.title),
                "workstation": workstation_name(node),
                "description": operation_description(map, template, &order_number, rubber_size),
                "sequence_id": index + 1,
                "batch_size": roll_count,
                "time_in_mins": 1,
                "status": "Pending",
            })
        })
        .collect();
    serde_json::json!({
        "production_item": item_code,
        "qty": template.kg,
        "description": format!("Zakaz: /{order_number}\nMahsulot: {product_name}\nRS map id: {}", map.id.trim()),
        "skip_transfer": 1,
        "transfer_material_against": "Job Card",
        "operations": operations,
    })
}

fn apparatus_nodes_in_flow_order(map: &ProductionMapDefinition) -> Vec<&ProductionMapNode> {
    let mut nodes: Vec<&ProductionMapNode> = map
        .nodes
        .iter()
        .filter(|node| node.kind == ProductionMapNodeKind::Apparatus)
        .collect();
    nodes.sort_by(|left, right| {
        node_depth(map, &left.id)
            .cmp(&node_depth(map, &right.id))
            .then_with(|| left.y.total_cmp(&right.y))
            .then_with(|| left.x.total_cmp(&right.x))
            .then_with(|| left.id.cmp(&right.id))
    });
    nodes
}

fn node_depth(map: &ProductionMapDefinition, node_id: &str) -> usize {
    let start_ids: Vec<&str> = map
        .nodes
        .iter()
        .filter(|node| node.kind == ProductionMapNodeKind::Start)
        .map(|node| node.id.as_str())
        .collect();
    let mut frontier = start_ids;
    let mut seen = std::collections::BTreeSet::new();
    let mut depth = 0;
    while !frontier.is_empty() {
        if frontier.contains(&node_id) {
            return depth;
        }
        let mut next = Vec::new();
        for current in frontier {
            if !seen.insert(current.to_string()) {
                continue;
            }
            for edge in map.edges.iter().filter(|edge| edge.from == current) {
                next.push(edge.to.as_str());
            }
        }
        frontier = next;
        depth += 1;
    }
    usize::MAX
}

fn workstation_name(node: &ProductionMapNode) -> String {
    if node.alternative_assigned_title.trim().is_empty() {
        node.title.trim().to_string()
    } else {
        node.alternative_assigned_title.trim().to_string()
    }
}

fn operation_name(title: &str) -> &'static str {
    let lower = title.to_lowercase();
    if lower.contains("laminat") {
        "Laminatsiya"
    } else if lower.contains("flex") || lower.contains("fleks") || lower.contains("flekso") {
        "Flexo"
    } else if lower.contains("pechat") {
        "Pechat"
    } else if lower.contains("paket") {
        "Paket"
    } else {
        "Ishlov"
    }
}

fn operation_description(
    map: &ProductionMapDefinition,
    template: &CalculateOrderTemplate,
    order_number: &str,
    rubber_size: i64,
) -> String {
    let width_mm = if template.width_mm > 0.0 {
        template.width_mm
    } else {
        map.width_mm.unwrap_or_default()
    };
    let roll_count = template.roll_count.or(map.roll_count).unwrap_or_default();
    let mut lines = vec![
        format!("Zakaz: /{}", order_number.trim()),
        format!("Material razmer: {} mm", number_text(width_mm)),
        format!("Rezina razmer: {rubber_size} mm"),
        format!("Qolib soni: {}", number_text(roll_count)),
        format!(
            "1 qavat: {} {}",
            template.first_layer_material.trim(),
            template.first_layer_micron.trim()
        ),
        format!(
            "2 qavat: {} {}",
            template.second_layer_material.trim(),
            template.second_layer_micron.trim()
        ),
    ];
    if !template.third_layer_material.trim().is_empty()
        || !template.third_layer_micron.trim().is_empty()
    {
        lines.push(format!(
            "3 qavat: {} {}",
            template.third_layer_material.trim(),
            template.third_layer_micron.trim()
        ));
    }
    lines.push(format!("RS map id: {}", map.id.trim()));
    lines.join("\n")
}

fn rubber_size_mm(template: &CalculateOrderTemplate) -> Option<i64> {
    let calculation = calculate(CalculateRequest {
        order_number: Some(first_non_empty([&template.order_number, &template.code])),
        product: Some(first_non_empty([&template.product, &template.name])),
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
    Some(i64::from(calculation.rubber_size_mm))
}

fn first_non_empty<const N: usize>(values: [&str; N]) -> String {
    values
        .into_iter()
        .map(str::trim)
        .find(|value| !value.is_empty())
        .unwrap_or("")
        .to_string()
}

fn number_text(value: f64) -> String {
    if value.fract().abs() < f64::EPSILON {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

fn now_erp_datetime() -> String {
    let now = time::OffsetDateTime::now_utc()
        .to_offset(time::UtcOffset::from_hms(5, 0, 0).expect("valid tashkent offset"));
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

fn encoded_path(path: &str) -> String {
    path.trim_start_matches(' ').replace(' ', "%20")
}

async fn decode_response<T: for<'de> Deserialize<'de>>(
    response: reqwest::Response,
) -> Result<T, ProductionOrderErpError> {
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| ProductionOrderErpError::WriteFailed(error.to_string()))?;
    if !status.is_success() {
        return Err(ProductionOrderErpError::WriteFailed(format!(
            "status {status}: {body}"
        )));
    }
    serde_json::from_str(&body)
        .map_err(|error| ProductionOrderErpError::WriteFailed(format!("{error}: {body}")))
}

#[derive(Debug, Deserialize)]
struct ResourceResponse<T> {
    data: T,
}

#[derive(Debug, Deserialize)]
struct ListResponse<T> {
    data: Vec<T>,
}

#[derive(Debug, Deserialize)]
struct NameRow {
    name: String,
}

#[derive(Debug, Deserialize)]
struct ItemRow {
    #[serde(default)]
    default_bom: String,
    #[serde(default)]
    stock_uom: String,
}

#[derive(Debug, Deserialize)]
struct WarehouseRow {
    #[serde(default)]
    company: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::production_map::{
        ProductionMapEdge, ProductionMapNode, ProductionMapNodeKind,
    };

    #[test]
    fn work_order_payload_maps_order_to_standard_erpnext_manufacturing_fields() {
        let map = ProductionMapDefinition {
            id: "zakaz-8756".to_string(),
            product_code: "ITEM-8756".to_string(),
            title: "vitagum vitamin zip paket".to_string(),
            code: "8756".to_string(),
            order_number: "8756".to_string(),
            roll_count: Some(7.0),
            width_mm: Some(630.0),
            nodes: vec![
                node("start", ProductionMapNodeKind::Start, "Start"),
                node(
                    "pechat",
                    ProductionMapNodeKind::Apparatus,
                    "7 ta rangli pechat - A",
                ),
                node("lam", ProductionMapNodeKind::Apparatus, "Laminatsiya 1 - A"),
                node("end", ProductionMapNodeKind::End, "End"),
            ],
            edges: vec![
                edge("start", "pechat"),
                edge("pechat", "lam"),
                edge("lam", "end"),
            ],
        };
        let template = CalculateOrderTemplate {
            name: "vitagum vitamin zip paket".to_string(),
            product: "vitagum vitamin zip paket".to_string(),
            item_code: "ITEM-8756".to_string(),
            width_mm: 630.0,
            waste_percent: 3.0,
            roll_count: Some(7.0),
            first_layer_material: "pet".to_string(),
            first_layer_micron: "12".to_string(),
            second_layer_material: "pe oq".to_string(),
            second_layer_micron: "50".to_string(),
            third_layer_material: "cpp".to_string(),
            third_layer_micron: "20".to_string(),
            kg: 1000.0,
            ..CalculateOrderTemplate::default()
        };

        let payload = build_work_order_payload(&map, &template);

        assert_eq!(payload["production_item"], "ITEM-8756");
        assert_eq!(payload["qty"], 1000.0);
        assert_eq!(
            payload["description"],
            "Zakaz: /8756\nMahsulot: vitagum vitamin zip paket\nRS map id: zakaz-8756"
        );
        assert_eq!(
            payload["operations"].as_array().expect("operations").len(),
            2
        );
        assert_eq!(payload["operations"][0]["operation"], "Pechat");
        assert_eq!(
            payload["operations"][0]["workstation"],
            "7 ta rangli pechat - A"
        );
        assert_eq!(payload["operations"][0]["sequence_id"], 1);
        assert_eq!(payload["operations"][0]["batch_size"], 7.0);
        assert_eq!(payload["operations"][0]["time_in_mins"], 1);
        assert_eq!(payload["operations"][1]["operation"], "Laminatsiya");
        assert_eq!(payload["operations"][1]["workstation"], "Laminatsiya 1 - A");
        assert_eq!(payload["operations"][1]["sequence_id"], 2);
        let description = payload["operations"][0]["description"]
            .as_str()
            .expect("operation description");
        assert!(description.contains("Material razmer: 630 mm"));
        assert!(description.contains("Rezina razmer: 650 mm"));
        assert!(description.contains("Qolib soni: 7"));
        assert!(description.contains("1 qavat: pet 12"));
        assert!(description.contains("2 qavat: pe oq 50"));
        assert!(description.contains("3 qavat: cpp 20"));
    }

    fn node(id: &str, kind: ProductionMapNodeKind, title: &str) -> ProductionMapNode {
        ProductionMapNode {
            id: id.to_string(),
            kind,
            title: title.to_string(),
            formula: None,
            role_code: String::new(),
            item_code: String::new(),
            qty_formula: String::new(),
            from_location: String::new(),
            to_location: String::new(),
            alternative_group_id: String::new(),
            alternative_group_label: String::new(),
            alternative_assigned_title: String::new(),
            x: 0.0,
            y: 0.0,
        }
    }

    fn edge(from: &str, to: &str) -> ProductionMapEdge {
        ProductionMapEdge {
            from: from.to_string(),
            to: to.to_string(),
            branch: String::new(),
        }
    }
}
