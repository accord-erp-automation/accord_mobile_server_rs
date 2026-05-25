use std::collections::{BTreeMap, BTreeSet, VecDeque};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductionMapDefinition {
    pub id: String,
    pub product_code: String,
    pub title: String,
    #[serde(default)]
    pub nodes: Vec<ProductionMapNode>,
    #[serde(default)]
    pub edges: Vec<ProductionMapEdge>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductionMapNode {
    pub id: String,
    pub kind: ProductionMapNodeKind,
    pub title: String,
    #[serde(default)]
    pub formula: Option<ProductionFormula>,
    #[serde(default)]
    pub role_code: String,
    #[serde(default)]
    pub item_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductionMapNodeKind {
    Start,
    Material,
    Formula,
    Task,
    Wait,
    Output,
    End,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductionFormula {
    pub target: String,
    pub expression: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductionMapEdge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductionMapProgram {
    pub map_id: String,
    pub product_code: String,
    pub operations: Vec<ProductionMapOperation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductionMapOperation {
    pub order: usize,
    pub node_id: String,
    pub op_code: String,
    pub args: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductionMapSaved {
    pub map: ProductionMapDefinition,
    pub program: ProductionMapProgram,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ProductionMapError {
    #[error("map id is required")]
    MissingId,
    #[error("product code is required")]
    MissingProductCode,
    #[error("map title is required")]
    MissingTitle,
    #[error("map needs one start node")]
    MissingStart,
    #[error("map needs one end node")]
    MissingEnd,
    #[error("duplicate node id: {0}")]
    DuplicateNode(String),
    #[error("edge references missing node: {0}")]
    MissingEdgeNode(String),
    #[error("map has a cycle")]
    Cycle,
    #[error("formula target is required")]
    MissingFormulaTarget,
    #[error("formula expression is required")]
    MissingFormulaExpression,
    #[error("store failed")]
    StoreFailed,
}

#[async_trait]
pub trait ProductionMapStorePort: Send + Sync {
    async fn maps(&self) -> Result<Vec<ProductionMapDefinition>, ProductionMapError>;
    async fn put_map(&self, map: ProductionMapDefinition) -> Result<(), ProductionMapError>;
}

pub struct MemoryProductionMapStore {
    maps: RwLock<BTreeMap<String, ProductionMapDefinition>>,
}

impl MemoryProductionMapStore {
    pub fn new() -> Self {
        Self {
            maps: RwLock::new(BTreeMap::new()),
        }
    }
}

impl Default for MemoryProductionMapStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ProductionMapStorePort for MemoryProductionMapStore {
    async fn maps(&self) -> Result<Vec<ProductionMapDefinition>, ProductionMapError> {
        Ok(self.maps.read().await.values().cloned().collect())
    }

    async fn put_map(&self, map: ProductionMapDefinition) -> Result<(), ProductionMapError> {
        self.maps.write().await.insert(map.id.clone(), map);
        Ok(())
    }
}

#[derive(Clone)]
pub struct ProductionMapService {
    store: std::sync::Arc<dyn ProductionMapStorePort>,
}

impl ProductionMapService {
    pub fn new(store: std::sync::Arc<dyn ProductionMapStorePort>) -> Self {
        Self { store }
    }

    pub async fn maps(&self) -> Result<Vec<ProductionMapSaved>, ProductionMapError> {
        let maps = self.store.maps().await?;
        maps.into_iter()
            .map(|map| {
                let program = compile_map(&map)?;
                Ok(ProductionMapSaved { map, program })
            })
            .collect()
    }

    pub async fn upsert_map(
        &self,
        mut map: ProductionMapDefinition,
    ) -> Result<ProductionMapSaved, ProductionMapError> {
        normalize_map(&mut map);
        let program = compile_map(&map)?;
        self.store.put_map(map.clone()).await?;
        Ok(ProductionMapSaved { map, program })
    }
}

pub fn compile_map(
    map: &ProductionMapDefinition,
) -> Result<ProductionMapProgram, ProductionMapError> {
    validate_map(map)?;
    let order = topological_order(map)?;
    let node_by_id: BTreeMap<&str, &ProductionMapNode> = map
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let mut operations = Vec::with_capacity(order.len());
    for (index, node_id) in order.into_iter().enumerate() {
        let node = node_by_id
            .get(node_id.as_str())
            .expect("topological order only contains known node ids");
        operations.push(compile_node(index + 1, node)?);
    }
    Ok(ProductionMapProgram {
        map_id: map.id.clone(),
        product_code: map.product_code.clone(),
        operations,
    })
}

fn normalize_map(map: &mut ProductionMapDefinition) {
    map.id = map.id.trim().to_ascii_lowercase();
    map.product_code = map.product_code.trim().to_string();
    map.title = map.title.trim().to_string();
    for node in &mut map.nodes {
        node.id = node.id.trim().to_ascii_lowercase();
        node.title = node.title.trim().to_string();
        node.role_code = node.role_code.trim().to_string();
        node.item_code = node.item_code.trim().to_string();
        if let Some(formula) = &mut node.formula {
            formula.target = formula.target.trim().to_string();
            formula.expression = formula.expression.trim().to_string();
        }
    }
    for edge in &mut map.edges {
        edge.from = edge.from.trim().to_ascii_lowercase();
        edge.to = edge.to.trim().to_ascii_lowercase();
    }
}

fn validate_map(map: &ProductionMapDefinition) -> Result<(), ProductionMapError> {
    if map.id.trim().is_empty() {
        return Err(ProductionMapError::MissingId);
    }
    if map.product_code.trim().is_empty() {
        return Err(ProductionMapError::MissingProductCode);
    }
    if map.title.trim().is_empty() {
        return Err(ProductionMapError::MissingTitle);
    }

    let mut ids = BTreeSet::new();
    let mut start_count = 0;
    let mut end_count = 0;
    for node in &map.nodes {
        if !ids.insert(node.id.as_str()) {
            return Err(ProductionMapError::DuplicateNode(node.id.clone()));
        }
        match node.kind {
            ProductionMapNodeKind::Start => start_count += 1,
            ProductionMapNodeKind::End => end_count += 1,
            ProductionMapNodeKind::Formula => {
                let Some(formula) = &node.formula else {
                    return Err(ProductionMapError::MissingFormulaExpression);
                };
                if formula.target.trim().is_empty() {
                    return Err(ProductionMapError::MissingFormulaTarget);
                }
                if formula.expression.trim().is_empty() {
                    return Err(ProductionMapError::MissingFormulaExpression);
                }
            }
            _ => {}
        }
    }
    if start_count != 1 {
        return Err(ProductionMapError::MissingStart);
    }
    if end_count != 1 {
        return Err(ProductionMapError::MissingEnd);
    }
    for edge in &map.edges {
        if !ids.contains(edge.from.as_str()) {
            return Err(ProductionMapError::MissingEdgeNode(edge.from.clone()));
        }
        if !ids.contains(edge.to.as_str()) {
            return Err(ProductionMapError::MissingEdgeNode(edge.to.clone()));
        }
    }
    Ok(())
}

fn topological_order(map: &ProductionMapDefinition) -> Result<Vec<String>, ProductionMapError> {
    let mut indegree = BTreeMap::<String, usize>::new();
    let mut outgoing = BTreeMap::<String, Vec<String>>::new();
    for node in &map.nodes {
        indegree.insert(node.id.clone(), 0);
        outgoing.insert(node.id.clone(), Vec::new());
    }
    for edge in &map.edges {
        *indegree
            .get_mut(&edge.to)
            .expect("validated edge target exists") += 1;
        outgoing
            .get_mut(&edge.from)
            .expect("validated edge source exists")
            .push(edge.to.clone());
    }

    let mut queue = indegree
        .iter()
        .filter_map(|(id, count)| (*count == 0).then_some(id.clone()))
        .collect::<VecDeque<_>>();
    let mut order = Vec::new();
    while let Some(id) = queue.pop_front() {
        order.push(id.clone());
        for child in outgoing.get(&id).into_iter().flatten() {
            let count = indegree
                .get_mut(child)
                .expect("validated child exists in indegree map");
            *count = count.saturating_sub(1);
            if *count == 0 {
                queue.push_back(child.clone());
            }
        }
    }
    if order.len() != map.nodes.len() {
        return Err(ProductionMapError::Cycle);
    }
    Ok(order)
}

fn compile_node(
    order: usize,
    node: &ProductionMapNode,
) -> Result<ProductionMapOperation, ProductionMapError> {
    let mut args = BTreeMap::new();
    args.insert("title".to_string(), node.title.clone());
    if !node.role_code.is_empty() {
        args.insert("role_code".to_string(), node.role_code.clone());
    }
    if !node.item_code.is_empty() {
        args.insert("item_code".to_string(), node.item_code.clone());
    }
    let op_code = match node.kind {
        ProductionMapNodeKind::Start => "start",
        ProductionMapNodeKind::Material => "require_material",
        ProductionMapNodeKind::Formula => {
            let Some(formula) = &node.formula else {
                return Err(ProductionMapError::MissingFormulaExpression);
            };
            args.insert("target".to_string(), formula.target.clone());
            args.insert("expression".to_string(), formula.expression.clone());
            "calculate"
        }
        ProductionMapNodeKind::Task => "create_task",
        ProductionMapNodeKind::Wait => "wait_dependency",
        ProductionMapNodeKind::Output => "produce_output",
        ProductionMapNodeKind::End => "end",
    };
    Ok(ProductionMapOperation {
        order,
        node_id: node.id.clone(),
        op_code: op_code.to_string(),
        args,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_map_turns_visual_nodes_into_ordered_operations() {
        let map = sample_map();
        let program = compile_map(&map).expect("compile");

        assert_eq!(program.map_id, "hotlunch-test");
        assert_eq!(program.operations.len(), 4);
        assert_eq!(program.operations[1].op_code, "calculate");
        assert_eq!(
            program.operations[1]
                .args
                .get("expression")
                .map(String::as_str),
            Some("order_qty * 1.08")
        );
        assert_eq!(program.operations[2].op_code, "create_task");
    }

    #[test]
    fn compile_map_rejects_cycles() {
        let mut map = sample_map();
        map.edges.push(ProductionMapEdge {
            from: "task".to_string(),
            to: "formula".to_string(),
        });

        assert_eq!(compile_map(&map), Err(ProductionMapError::Cycle));
    }

    fn sample_map() -> ProductionMapDefinition {
        ProductionMapDefinition {
            id: "hotlunch-test".to_string(),
            product_code: "HOTLUNCH".to_string(),
            title: "Hotlunch test".to_string(),
            nodes: vec![
                ProductionMapNode {
                    id: "start".to_string(),
                    kind: ProductionMapNodeKind::Start,
                    title: "Start".to_string(),
                    formula: None,
                    role_code: String::new(),
                    item_code: String::new(),
                },
                ProductionMapNode {
                    id: "formula".to_string(),
                    kind: ProductionMapNodeKind::Formula,
                    title: "CPP hisob".to_string(),
                    formula: Some(ProductionFormula {
                        target: "cpp_kg".to_string(),
                        expression: "order_qty * 1.08".to_string(),
                    }),
                    role_code: String::new(),
                    item_code: "CPP".to_string(),
                },
                ProductionMapNode {
                    id: "task".to_string(),
                    kind: ProductionMapNodeKind::Task,
                    title: "Rezkaga yuborish".to_string(),
                    formula: None,
                    role_code: "rezkachi".to_string(),
                    item_code: String::new(),
                },
                ProductionMapNode {
                    id: "end".to_string(),
                    kind: ProductionMapNodeKind::End,
                    title: "End".to_string(),
                    formula: None,
                    role_code: String::new(),
                    item_code: String::new(),
                },
            ],
            edges: vec![
                ProductionMapEdge {
                    from: "start".to_string(),
                    to: "formula".to_string(),
                },
                ProductionMapEdge {
                    from: "formula".to_string(),
                    to: "task".to_string(),
                },
                ProductionMapEdge {
                    from: "task".to_string(),
                    to: "end".to_string(),
                },
            ],
        }
    }
}
