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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductionMapRunRequest {
    #[serde(default)]
    pub map_id: String,
    #[serde(default)]
    pub product_code: String,
    pub order_qty: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductionTaskDraft {
    pub order: usize,
    pub node_id: String,
    pub task_kind: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub role_code: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub item_code: String,
    pub qty: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductionMapRunResult {
    pub map_id: String,
    pub product_code: String,
    pub order_qty: f64,
    pub variables: BTreeMap<String, f64>,
    pub tasks: Vec<ProductionTaskDraft>,
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
    #[error("invalid formula target: {0}")]
    InvalidFormulaTarget(String),
    #[error("invalid formula expression: {0}")]
    InvalidFormulaExpression(String),
    #[error("map not found")]
    MapNotFound,
    #[error("order quantity must be positive")]
    InvalidOrderQty,
    #[error("unknown formula variable: {0}")]
    UnknownFormulaVariable(String),
    #[error("formula division by zero")]
    FormulaDivisionByZero,
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

    pub async fn run_map(
        &self,
        input: ProductionMapRunRequest,
    ) -> Result<ProductionMapRunResult, ProductionMapError> {
        if input.order_qty <= 0.0 {
            return Err(ProductionMapError::InvalidOrderQty);
        }
        let map_id = input.map_id.trim().to_ascii_lowercase();
        let product_code = input.product_code.trim();
        let maps = self.store.maps().await?;
        let Some(map) = maps.into_iter().find(|map| {
            (!map_id.is_empty() && map.id == map_id)
                || (!product_code.is_empty() && map.product_code == product_code)
        }) else {
            return Err(ProductionMapError::MapNotFound);
        };
        run_map(&map, input.order_qty)
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
                validate_formula_target(&formula.target)?;
                validate_formula_expression(&formula.expression)?;
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

fn validate_formula_target(target: &str) -> Result<(), ProductionMapError> {
    if is_identifier(target.trim()) {
        Ok(())
    } else {
        Err(ProductionMapError::InvalidFormulaTarget(target.to_string()))
    }
}

fn validate_formula_expression(expression: &str) -> Result<(), ProductionMapError> {
    let mut parser = FormulaParser::new(expression);
    parser.parse_expression()?;
    parser.skip_whitespace();
    if parser.is_eof() {
        Ok(())
    } else {
        Err(ProductionMapError::InvalidFormulaExpression(
            expression.to_string(),
        ))
    }
}

fn evaluate_formula(
    expression: &str,
    variables: &BTreeMap<String, f64>,
) -> Result<f64, ProductionMapError> {
    let mut parser = FormulaParser::new(expression);
    let value = parser.evaluate_expression(variables)?;
    parser.skip_whitespace();
    if parser.is_eof() {
        Ok(value)
    } else {
        Err(ProductionMapError::InvalidFormulaExpression(
            expression.to_string(),
        ))
    }
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

struct FormulaParser<'a> {
    input: &'a str,
    position: usize,
}

impl<'a> FormulaParser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, position: 0 }
    }

    fn parse_expression(&mut self) -> Result<(), ProductionMapError> {
        self.parse_term()?;
        loop {
            self.skip_whitespace();
            if self.consume('+') || self.consume('-') {
                self.parse_term()?;
            } else {
                return Ok(());
            }
        }
    }

    fn evaluate_expression(
        &mut self,
        variables: &BTreeMap<String, f64>,
    ) -> Result<f64, ProductionMapError> {
        let mut value = self.evaluate_term(variables)?;
        loop {
            self.skip_whitespace();
            if self.consume('+') {
                value += self.evaluate_term(variables)?;
            } else if self.consume('-') {
                value -= self.evaluate_term(variables)?;
            } else {
                return Ok(value);
            }
        }
    }

    fn parse_term(&mut self) -> Result<(), ProductionMapError> {
        self.parse_factor()?;
        loop {
            self.skip_whitespace();
            if self.consume('*') || self.consume('/') {
                self.parse_factor()?;
            } else {
                return Ok(());
            }
        }
    }

    fn evaluate_term(
        &mut self,
        variables: &BTreeMap<String, f64>,
    ) -> Result<f64, ProductionMapError> {
        let mut value = self.evaluate_factor(variables)?;
        loop {
            self.skip_whitespace();
            if self.consume('*') {
                value *= self.evaluate_factor(variables)?;
            } else if self.consume('/') {
                let divisor = self.evaluate_factor(variables)?;
                if divisor == 0.0 {
                    return Err(ProductionMapError::FormulaDivisionByZero);
                }
                value /= divisor;
            } else {
                return Ok(value);
            }
        }
    }

    fn parse_factor(&mut self) -> Result<(), ProductionMapError> {
        self.skip_whitespace();
        if self.consume('-') {
            return self.parse_factor();
        }
        if self.consume('(') {
            self.parse_expression()?;
            self.skip_whitespace();
            return if self.consume(')') {
                Ok(())
            } else {
                self.invalid()
            };
        }
        if self.parse_identifier() || self.parse_number() {
            Ok(())
        } else {
            self.invalid()
        }
    }

    fn evaluate_factor(
        &mut self,
        variables: &BTreeMap<String, f64>,
    ) -> Result<f64, ProductionMapError> {
        self.skip_whitespace();
        if self.consume('-') {
            return Ok(-self.evaluate_factor(variables)?);
        }
        if self.consume('(') {
            let value = self.evaluate_expression(variables)?;
            self.skip_whitespace();
            return if self.consume(')') {
                Ok(value)
            } else {
                self.invalid()
            };
        }
        if let Some(identifier) = self.read_identifier() {
            return variables
                .get(&identifier)
                .copied()
                .ok_or(ProductionMapError::UnknownFormulaVariable(identifier));
        }
        if let Some(number) = self.read_number() {
            return Ok(number);
        }
        self.invalid()
    }

    fn parse_identifier(&mut self) -> bool {
        self.read_identifier().is_some()
    }

    fn read_identifier(&mut self) -> Option<String> {
        let start = self.position;
        while let Some(ch) = self.peek() {
            if self.position == start {
                if ch.is_ascii_alphabetic() || ch == '_' {
                    self.position += ch.len_utf8();
                } else {
                    break;
                }
            } else if ch.is_ascii_alphanumeric() || ch == '_' {
                self.position += ch.len_utf8();
            } else {
                break;
            }
        }
        (self.position > start).then(|| self.input[start..self.position].to_string())
    }

    fn parse_number(&mut self) -> bool {
        self.read_number().is_some()
    }

    fn read_number(&mut self) -> Option<f64> {
        let start = self.position;
        while matches!(self.peek(), Some(ch) if ch.is_ascii_digit()) {
            self.position += 1;
        }
        if self.consume('.') {
            while matches!(self.peek(), Some(ch) if ch.is_ascii_digit()) {
                self.position += 1;
            }
        }
        (self.position > start)
            .then(|| self.input[start..self.position].parse::<f64>().ok())
            .flatten()
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(ch) if ch.is_ascii_whitespace()) {
            self.position += 1;
        }
    }

    fn consume(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.position += expected.len_utf8();
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<char> {
        self.input[self.position..].chars().next()
    }

    fn is_eof(&self) -> bool {
        self.position >= self.input.len()
    }

    fn invalid<T>(&self) -> Result<T, ProductionMapError> {
        Err(ProductionMapError::InvalidFormulaExpression(
            self.input.to_string(),
        ))
    }
}

pub fn run_map(
    map: &ProductionMapDefinition,
    order_qty: f64,
) -> Result<ProductionMapRunResult, ProductionMapError> {
    if order_qty <= 0.0 {
        return Err(ProductionMapError::InvalidOrderQty);
    }
    let program = compile_map(map)?;
    let node_by_id: BTreeMap<&str, &ProductionMapNode> = map
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let mut variables = BTreeMap::from([("order_qty".to_string(), order_qty)]);
    let mut tasks = Vec::new();
    for operation in &program.operations {
        let node = node_by_id
            .get(operation.node_id.as_str())
            .expect("compiled operation only contains known node ids");
        match node.kind {
            ProductionMapNodeKind::Formula => {
                let Some(formula) = &node.formula else {
                    return Err(ProductionMapError::MissingFormulaExpression);
                };
                let value = evaluate_formula(&formula.expression, &variables)?;
                variables.insert(formula.target.clone(), value);
            }
            ProductionMapNodeKind::Material
            | ProductionMapNodeKind::Task
            | ProductionMapNodeKind::Wait
            | ProductionMapNodeKind::Output => tasks.push(ProductionTaskDraft {
                order: operation.order,
                node_id: node.id.clone(),
                task_kind: operation.op_code.clone(),
                title: node.title.clone(),
                role_code: node.role_code.clone(),
                item_code: node.item_code.clone(),
                qty: order_qty,
            }),
            ProductionMapNodeKind::Start | ProductionMapNodeKind::End => {}
        }
    }
    Ok(ProductionMapRunResult {
        map_id: map.id.clone(),
        product_code: map.product_code.clone(),
        order_qty,
        variables,
        tasks,
    })
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

    #[test]
    fn compile_map_rejects_invalid_formula_expression() {
        let mut map = sample_map();
        map.nodes[1].formula = Some(ProductionFormula {
            target: "cpp_kg".to_string(),
            expression: "order_qty; drop".to_string(),
        });

        assert_eq!(
            compile_map(&map),
            Err(ProductionMapError::InvalidFormulaExpression(
                "order_qty; drop".to_string()
            ))
        );
    }

    #[test]
    fn run_map_evaluates_formulas_and_generates_task_drafts() {
        let result = run_map(&sample_map(), 100.0).expect("run map");

        assert_eq!(result.map_id, "hotlunch-test");
        assert_eq!(result.variables.get("order_qty"), Some(&100.0));
        assert_eq!(result.variables.get("cpp_kg"), Some(&108.0));
        assert_eq!(result.tasks.len(), 1);
        assert_eq!(result.tasks[0].task_kind, "create_task");
        assert_eq!(result.tasks[0].role_code, "rezkachi");
        assert_eq!(result.tasks[0].qty, 100.0);
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
