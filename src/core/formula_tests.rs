use crate::core::formula::{CalculateRequest, LayerInput, calculate};

#[test]
fn calculates_formula_with_waste_and_rounding() {
    let result = calculate(CalculateRequest {
        kg: Some(300.0),
        width_mm: Some(530.0),
        first_layer: LayerInput::new("pet", "12"),
        second_layer: LayerInput::new("pe oq", "30"),
        ..CalculateRequest::default()
    })
    .expect("calculate");

    assert_eq!(result.results.len(), 1);
    assert_eq!(result.results[0].rounded_length, 12000.0);
    assert!((result.results[0].base_length - 11320.7547).abs() < 0.001);
    assert!((result.results[0].waste_length - 566.0377).abs() < 0.001);
}

#[test]
fn calculates_alternative_material_variants() {
    let result = calculate(CalculateRequest {
        kg: Some(300.0),
        width_mm: Some(530.0),
        first_layer: LayerInput::new("pet", "12"),
        second_layer: LayerInput::new("pe oq yoki mcp", "30"),
        ..CalculateRequest::default()
    })
    .expect("calculate");

    let lengths = result
        .results
        .into_iter()
        .map(|result| result.rounded_length)
        .collect::<Vec<_>>();
    assert_eq!(lengths, vec![12000.0, 14000.0]);
}

#[test]
fn parses_material_display_when_layers_are_empty() {
    let result = calculate(CalculateRequest {
        kg: Some(3000.0),
        width_mm: Some(635.0),
        material_display: Some("pet 12 + oppm/pe pr 20/30".to_string()),
        ..CalculateRequest::default()
    })
    .expect("calculate");

    assert_eq!(result.results[0].rounded_length, 74500.0);
    assert_eq!(result.layers[0].material, "pet");
    assert_eq!(result.layers[1].material, "oppm/pe pr");
}
