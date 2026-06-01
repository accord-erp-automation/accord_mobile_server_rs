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
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub qty_formula: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub from_location: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub to_location: String,
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductionMapNodeKind {
    Start,
    Location,
    Material,
    Formula,
    Condition,
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
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub branch: String,
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
    #[serde(default)]
    pub variables: BTreeMap<String, f64>,
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
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub from_location: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub to_location: String,
    pub qty: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductionMapRunResult {
    pub map_id: String,
    pub product_code: String,
    pub order_qty: f64,
    pub variables: BTreeMap<String, f64>,
    pub tasks: Vec<ProductionTaskDraft>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub visited_node_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub awaiting_node_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub awaiting_variable: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub awaiting_expression: String,
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
    #[error("node quantity must be positive: {0}")]
    InvalidNodeQty(String),
    #[error("invalid location: {0}")]
    InvalidLocation(String),
    #[error("unknown formula variable: {0}")]
    UnknownFormulaVariable(String),
    #[error("formula division by zero")]
    FormulaDivisionByZero,
    #[error("condition needs true and false branches")]
    MissingConditionBranch,
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
        run_map_with_variables(&map, input.order_qty, input.variables)
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
        node.qty_formula = node.qty_formula.trim().to_string();
        node.from_location = node.from_location.trim().to_string();
        node.to_location = node.to_location.trim().to_string();
        if !node.x.is_finite() {
            node.x = 0.0;
        }
        if !node.y.is_finite() {
            node.y = 0.0;
        }
        if let Some(formula) = &mut node.formula {
            formula.target = formula.target.trim().to_string();
            formula.expression = formula.expression.trim().to_string();
        }
    }
    for edge in &mut map.edges {
        edge.from = edge.from.trim().to_ascii_lowercase();
        edge.to = edge.to.trim().to_ascii_lowercase();
        edge.branch = normalize_branch(&edge.branch);
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
            ProductionMapNodeKind::Condition => {
                let Some(formula) = &node.formula else {
                    return Err(ProductionMapError::MissingFormulaExpression);
                };
                if formula.expression.trim().is_empty() {
                    return Err(ProductionMapError::MissingFormulaExpression);
                }
                validate_condition_expression(&formula.expression)?;
            }
            ProductionMapNodeKind::Location => {}
            ProductionMapNodeKind::Material
            | ProductionMapNodeKind::Task
            | ProductionMapNodeKind::Wait
            | ProductionMapNodeKind::Output => {
                if !node.qty_formula.trim().is_empty() {
                    validate_formula_expression(&node.qty_formula)?;
                }
            }
        }
        validate_location_ref(&node.from_location)?;
        validate_location_ref(&node.to_location)?;
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
    for node in &map.nodes {
        if node.kind != ProductionMapNodeKind::Condition {
            continue;
        }
        let mut has_true = false;
        let mut has_false = false;
        for edge in map.edges.iter().filter(|edge| edge.from == node.id) {
            match normalize_branch(&edge.branch).as_str() {
                "true" => has_true = true,
                "false" => has_false = true,
                _ => {}
            }
        }
        if !has_true || !has_false {
            return Err(ProductionMapError::MissingConditionBranch);
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

fn validate_location_ref(location: &str) -> Result<(), ProductionMapError> {
    let location = location.trim();
    if location.is_empty() {
        return Ok(());
    }
    let valid = location.len() <= 120
        && location.chars().any(char::is_alphanumeric)
        && location.chars().all(|ch| {
            ch.is_alphanumeric()
                || ch.is_whitespace()
                || matches!(ch, '-' | '_' | '.' | '/' | '(' | ')')
        });
    if valid {
        Ok(())
    } else {
        Err(ProductionMapError::InvalidLocation(location.to_string()))
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

fn validate_condition_expression(expression: &str) -> Result<(), ProductionMapError> {
    evaluate_condition(expression, &BTreeMap::new())
        .map(|_| ())
        .or_else(|error| {
            if matches!(error, ProductionMapError::UnknownFormulaVariable(_)) {
                Ok(())
            } else {
                Err(error)
            }
        })
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

fn evaluate_condition(
    expression: &str,
    variables: &BTreeMap<String, f64>,
) -> Result<bool, ProductionMapError> {
    if let Some((left, operator, right)) = split_condition(expression) {
        let left = evaluate_formula(left, variables)?;
        let right = evaluate_formula(right, variables)?;
        return match operator {
            ">" => Ok(left > right),
            ">=" => Ok(left >= right),
            "<" => Ok(left < right),
            "<=" => Ok(left <= right),
            "==" => Ok((left - right).abs() < f64::EPSILON),
            "!=" => Ok((left - right).abs() >= f64::EPSILON),
            _ => Err(ProductionMapError::InvalidFormulaExpression(
                expression.to_string(),
            )),
        };
    }
    Ok(evaluate_formula(expression, variables)? != 0.0)
}

fn split_condition(expression: &str) -> Option<(&str, &str, &str)> {
    let mut depth = 0usize;
    let bytes = expression.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            _ if depth == 0 => {
                for operator in [">=", "<=", "==", "!=", ">", "<"] {
                    if expression[index..].starts_with(operator) {
                        let left = expression[..index].trim();
                        let right = expression[index + operator.len()..].trim();
                        if !left.is_empty() && !right.is_empty() {
                            return Some((left, operator, right));
                        }
                    }
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

fn normalize_branch(branch: &str) -> String {
    match branch.trim().to_ascii_lowercase().as_str() {
        "ha" | "yes" | "true" | "1" => "true".to_string(),
        "yo'q" | "yoq" | "no" | "false" | "0" => "false".to_string(),
        value => value.to_string(),
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
    run_map_with_variables(map, order_qty, BTreeMap::new())
}

pub fn run_map_with_variables(
    map: &ProductionMapDefinition,
    order_qty: f64,
    run_variables: BTreeMap<String, f64>,
) -> Result<ProductionMapRunResult, ProductionMapError> {
    if order_qty <= 0.0 {
        return Err(ProductionMapError::InvalidOrderQty);
    }
    compile_map(map)?;
    let node_by_id: BTreeMap<&str, &ProductionMapNode> = map
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let mut outgoing = BTreeMap::<&str, Vec<&ProductionMapEdge>>::new();
    for edge in &map.edges {
        outgoing.entry(edge.from.as_str()).or_default().push(edge);
    }
    let mut variables = input_variables(order_qty, run_variables);
    let mut tasks = Vec::new();
    let Some(mut current_id) = map
        .nodes
        .iter()
        .find(|node| node.kind == ProductionMapNodeKind::Start)
        .map(|node| node.id.as_str())
    else {
        return Err(ProductionMapError::MissingStart);
    };
    let mut visited = BTreeSet::new();
    let mut visited_node_ids = Vec::new();
    while visited.insert(current_id.to_string()) {
        let node = node_by_id
            .get(current_id)
            .expect("compiled map only contains known node ids");
        visited_node_ids.push(node.id.clone());
        if node.kind == ProductionMapNodeKind::End {
            break;
        }
        match node.kind {
            ProductionMapNodeKind::Formula => {
                let Some(formula) = &node.formula else {
                    return Err(ProductionMapError::MissingFormulaExpression);
                };
                let value = evaluate_formula(&formula.expression, &variables)?;
                variables.insert(formula.target.clone(), value);
            }
            ProductionMapNodeKind::Condition => {
                let Some(formula) = &node.formula else {
                    return Err(ProductionMapError::MissingFormulaExpression);
                };
                let result = match evaluate_condition(&formula.expression, &variables) {
                    Ok(result) => result,
                    Err(ProductionMapError::UnknownFormulaVariable(variable)) => {
                        return Ok(ProductionMapRunResult {
                            map_id: map.id.clone(),
                            product_code: map.product_code.clone(),
                            order_qty,
                            variables,
                            tasks,
                            visited_node_ids,
                            awaiting_node_id: node.id.clone(),
                            awaiting_variable: variable,
                            awaiting_expression: formula.expression.clone(),
                        });
                    }
                    Err(error) => return Err(error),
                };
                variables.insert(node.id.clone(), if result { 1.0 } else { 0.0 });
            }
            ProductionMapNodeKind::Location => {}
            ProductionMapNodeKind::Material
            | ProductionMapNodeKind::Task
            | ProductionMapNodeKind::Wait
            | ProductionMapNodeKind::Output => {
                let qty = node_qty(node, order_qty, &variables)?;
                tasks.push(ProductionTaskDraft {
                    order: tasks.len() + 1,
                    node_id: node.id.clone(),
                    task_kind: compile_node(tasks.len() + 1, node)?.op_code,
                    title: node.title.clone(),
                    role_code: node.role_code.clone(),
                    item_code: node.item_code.clone(),
                    from_location: node.from_location.clone(),
                    to_location: node.to_location.clone(),
                    qty,
                })
            }
            ProductionMapNodeKind::Start | ProductionMapNodeKind::End => {}
        }
        let edges = outgoing.get(current_id).cloned().unwrap_or_default();
        if node.kind == ProductionMapNodeKind::Condition {
            let branch = if variables.get(&node.id).copied().unwrap_or(0.0) != 0.0 {
                "true"
            } else {
                "false"
            };
            let Some(next) = edges
                .into_iter()
                .find(|edge| normalize_branch(&edge.branch) == branch)
            else {
                return Err(ProductionMapError::MissingConditionBranch);
            };
            current_id = next.to.as_str();
        } else {
            let Some(next) = edges.first() else {
                break;
            };
            current_id = next.to.as_str();
        }
    }
    Ok(ProductionMapRunResult {
        map_id: map.id.clone(),
        product_code: map.product_code.clone(),
        order_qty,
        variables,
        tasks,
        visited_node_ids,
        awaiting_node_id: String::new(),
        awaiting_variable: String::new(),
        awaiting_expression: String::new(),
    })
}

fn input_variables(order_qty: f64, mut variables: BTreeMap<String, f64>) -> BTreeMap<String, f64> {
    variables.insert("order_qty".to_string(), order_qty);
    variables
}

fn node_qty(
    node: &ProductionMapNode,
    order_qty: f64,
    variables: &BTreeMap<String, f64>,
) -> Result<f64, ProductionMapError> {
    let qty = if node.qty_formula.trim().is_empty() {
        order_qty
    } else {
        evaluate_formula(&node.qty_formula, variables)?
    };
    if qty.is_finite() && qty > 0.0 {
        Ok(qty)
    } else {
        Err(ProductionMapError::InvalidNodeQty(node.id.clone()))
    }
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
    if !node.qty_formula.is_empty() {
        args.insert("qty_formula".to_string(), node.qty_formula.clone());
    }
    if !node.from_location.is_empty() {
        args.insert("from_location".to_string(), node.from_location.clone());
    }
    if !node.to_location.is_empty() {
        args.insert("to_location".to_string(), node.to_location.clone());
    }
    let op_code = match node.kind {
        ProductionMapNodeKind::Start => "start",
        ProductionMapNodeKind::Location => "warehouse_location",
        ProductionMapNodeKind::Material => "require_material",
        ProductionMapNodeKind::Formula => {
            let Some(formula) = &node.formula else {
                return Err(ProductionMapError::MissingFormulaExpression);
            };
            args.insert("target".to_string(), formula.target.clone());
            args.insert("expression".to_string(), formula.expression.clone());
            "calculate"
        }
        ProductionMapNodeKind::Condition => {
            let Some(formula) = &node.formula else {
                return Err(ProductionMapError::MissingFormulaExpression);
            };
            args.insert("expression".to_string(), formula.expression.clone());
            "condition"
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
    fn compile_map_accepts_location_markers_without_task_drafts() {
        let mut map = sample_map();
        map.nodes.insert(
            1,
            ProductionMapNode {
                id: "cpp_warehouse".to_string(),
                kind: ProductionMapNodeKind::Location,
                title: "CPP ombor".to_string(),
                formula: None,
                role_code: String::new(),
                item_code: String::new(),
                qty_formula: String::new(),
                from_location: String::new(),
                to_location: String::new(),
                x: 0.0,
                y: 0.0,
            },
        );
        map.edges[0].to = "cpp_warehouse".to_string();
        map.edges.insert(
            1,
            ProductionMapEdge {
                from: "cpp_warehouse".to_string(),
                to: "formula".to_string(),
                branch: String::new(),
            },
        );

        let program = compile_map(&map).expect("compile");
        assert_eq!(program.operations[1].op_code, "warehouse_location");

        let result = run_map(&map, 100.0).expect("run map");
        assert_eq!(result.tasks.len(), 1);
        assert_eq!(result.tasks[0].node_id, "task");
    }

    #[test]
    fn compile_map_rejects_cycles() {
        let mut map = sample_map();
        map.edges.push(ProductionMapEdge {
            from: "task".to_string(),
            to: "formula".to_string(),
            branch: String::new(),
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
        assert_eq!(result.tasks[0].qty, 108.0);
        assert_eq!(result.tasks[0].from_location, "CPP ombor");
        assert_eq!(result.tasks[0].to_location, "Rezka apparat");
        assert_eq!(result.visited_node_ids, ["start", "formula", "task", "end"]);
    }

    #[test]
    fn run_map_follows_condition_branch() {
        let result = run_map(&condition_map(), 120.0).expect("run map");

        assert_eq!(result.variables.get("large_order"), Some(&1.0));
        assert_eq!(result.tasks.len(), 1);
        assert_eq!(result.tasks[0].node_id, "large_task");

        let result = run_map(&condition_map(), 60.0).expect("run map");
        assert_eq!(result.variables.get("large_order"), Some(&0.0));
        assert_eq!(result.tasks.len(), 1);
        assert_eq!(result.tasks[0].node_id, "small_task");
    }

    #[test]
    fn run_map_conditions_can_use_runtime_variables() {
        let mut map = condition_map();
        map.nodes[1].formula = Some(ProductionFormula {
            target: String::new(),
            expression: "pechat_ok == 1".to_string(),
        });

        let result = run_map_with_variables(
            &map,
            100.0,
            BTreeMap::from([("pechat_ok".to_string(), 1.0)]),
        )
        .expect("run map with ok result");

        assert_eq!(result.variables.get("pechat_ok"), Some(&1.0));
        assert!(result.awaiting_variable.is_empty());
        assert_eq!(result.tasks[0].node_id, "large_task");

        let result = run_map_with_variables(
            &map,
            100.0,
            BTreeMap::from([("pechat_ok".to_string(), 0.0)]),
        )
        .expect("run map with failed result");
        assert_eq!(result.tasks[0].node_id, "small_task");
    }

    #[test]
    fn run_map_stops_at_condition_when_runtime_variable_is_missing() {
        let mut map = condition_map();
        map.nodes[1].formula = Some(ProductionFormula {
            target: String::new(),
            expression: "pechat_ok == 1".to_string(),
        });

        let result = run_map(&map, 100.0).expect("run map waiting for variable");

        assert_eq!(result.tasks.len(), 0);
        assert_eq!(result.awaiting_node_id, "large_order");
        assert_eq!(result.awaiting_variable, "pechat_ok");
        assert_eq!(result.awaiting_expression, "pechat_ok == 1");
        assert_eq!(result.visited_node_ids, ["start", "large_order"]);
    }

    #[test]
    fn run_map_rejects_non_positive_node_qty() {
        let mut map = sample_map();
        map.nodes[2].qty_formula = "order_qty - 100".to_string();

        assert_eq!(
            run_map(&map, 100.0),
            Err(ProductionMapError::InvalidNodeQty("task".to_string()))
        );
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
                    qty_formula: String::new(),
                    from_location: String::new(),
                    to_location: String::new(),
                    x: 0.0,
                    y: 0.0,
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
                    qty_formula: String::new(),
                    from_location: String::new(),
                    to_location: String::new(),
                    x: 0.0,
                    y: 0.0,
                },
                ProductionMapNode {
                    id: "task".to_string(),
                    kind: ProductionMapNodeKind::Task,
                    title: "Rezkaga yuborish".to_string(),
                    formula: None,
                    role_code: "rezkachi".to_string(),
                    item_code: String::new(),
                    qty_formula: "cpp_kg".to_string(),
                    from_location: "CPP ombor".to_string(),
                    to_location: "Rezka apparat".to_string(),
                    x: 0.0,
                    y: 0.0,
                },
                ProductionMapNode {
                    id: "end".to_string(),
                    kind: ProductionMapNodeKind::End,
                    title: "End".to_string(),
                    formula: None,
                    role_code: String::new(),
                    item_code: String::new(),
                    qty_formula: String::new(),
                    from_location: String::new(),
                    to_location: String::new(),
                    x: 0.0,
                    y: 0.0,
                },
            ],
            edges: vec![
                ProductionMapEdge {
                    from: "start".to_string(),
                    to: "formula".to_string(),
                    branch: String::new(),
                },
                ProductionMapEdge {
                    from: "formula".to_string(),
                    to: "task".to_string(),
                    branch: String::new(),
                },
                ProductionMapEdge {
                    from: "task".to_string(),
                    to: "end".to_string(),
                    branch: String::new(),
                },
            ],
        }
    }

    fn condition_map() -> ProductionMapDefinition {
        ProductionMapDefinition {
            id: "branch-test".to_string(),
            product_code: "HOTLUNCH".to_string(),
            title: "Branch test".to_string(),
            nodes: vec![
                ProductionMapNode {
                    id: "start".to_string(),
                    kind: ProductionMapNodeKind::Start,
                    title: "Start".to_string(),
                    formula: None,
                    role_code: String::new(),
                    item_code: String::new(),
                    qty_formula: String::new(),
                    from_location: String::new(),
                    to_location: String::new(),
                    x: 0.0,
                    y: 0.0,
                },
                ProductionMapNode {
                    id: "large_order".to_string(),
                    kind: ProductionMapNodeKind::Condition,
                    title: "Katta partiyami".to_string(),
                    formula: Some(ProductionFormula {
                        target: String::new(),
                        expression: "order_qty >= 100".to_string(),
                    }),
                    role_code: String::new(),
                    item_code: String::new(),
                    qty_formula: String::new(),
                    from_location: String::new(),
                    to_location: String::new(),
                    x: 0.0,
                    y: 0.0,
                },
                ProductionMapNode {
                    id: "large_task".to_string(),
                    kind: ProductionMapNodeKind::Task,
                    title: "Katta partiya".to_string(),
                    formula: None,
                    role_code: "rezkachi".to_string(),
                    item_code: String::new(),
                    qty_formula: "order_qty / 6".to_string(),
                    from_location: "CPP ombor".to_string(),
                    to_location: "Rezka apparat".to_string(),
                    x: 0.0,
                    y: 0.0,
                },
                ProductionMapNode {
                    id: "small_task".to_string(),
                    kind: ProductionMapNodeKind::Task,
                    title: "Oddiy partiya".to_string(),
                    formula: None,
                    role_code: "operator".to_string(),
                    item_code: String::new(),
                    qty_formula: String::new(),
                    from_location: String::new(),
                    to_location: String::new(),
                    x: 0.0,
                    y: 0.0,
                },
                ProductionMapNode {
                    id: "end".to_string(),
                    kind: ProductionMapNodeKind::End,
                    title: "End".to_string(),
                    formula: None,
                    role_code: String::new(),
                    item_code: String::new(),
                    qty_formula: String::new(),
                    from_location: String::new(),
                    to_location: String::new(),
                    x: 0.0,
                    y: 0.0,
                },
            ],
            edges: vec![
                ProductionMapEdge {
                    from: "start".to_string(),
                    to: "large_order".to_string(),
                    branch: String::new(),
                },
                ProductionMapEdge {
                    from: "large_order".to_string(),
                    to: "large_task".to_string(),
                    branch: "true".to_string(),
                },
                ProductionMapEdge {
                    from: "large_order".to_string(),
                    to: "small_task".to_string(),
                    branch: "false".to_string(),
                },
                ProductionMapEdge {
                    from: "large_task".to_string(),
                    to: "end".to_string(),
                    branch: String::new(),
                },
                ProductionMapEdge {
                    from: "small_task".to_string(),
                    to: "end".to_string(),
                    branch: String::new(),
                },
            ],
        }
    }
}
