use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

const SUPPORTED_ELEMENTS: &[&str] = &[
    "Al", "B", "C", "Cr", "Cu", "Fe", "Mn", "Mo", "Nb", "Ni", "P", "S", "Si", "Ti", "V",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompositionUnit {
    PercentMass,
    MassFraction,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CompositionInput {
    pub values: BTreeMap<String, f64>,
    pub unit: CompositionUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CarbonEquivalentFormula {
    Iiw,
    Pcm,
}

impl CarbonEquivalentFormula {
    fn id(self) -> &'static str {
        match self {
            Self::Iiw => "carbon-equivalent.iiw.v1",
            Self::Pcm => "carbon-equivalent.pcm.v1",
        }
    }

    fn expression(self) -> &'static str {
        match self {
            Self::Iiw => "C + Mn/6 + (Cr + Mo + V)/5 + (Ni + Cu)/15",
            Self::Pcm => "C + Si/30 + (Mn + Cu + Cr)/20 + Ni/60 + Mo/15 + V/10 + 5B",
        }
    }

    fn required_elements(self) -> &'static [&'static str] {
        match self {
            Self::Iiw => &["C", "Mn", "Cr", "Mo", "V", "Ni", "Cu"],
            Self::Pcm => &["C", "Si", "Mn", "Cu", "Cr", "Ni", "Mo", "V", "B"],
        }
    }

    fn applicability_note(self) -> &'static str {
        match self {
            Self::Iiw => "IIW carbon equivalent is a weldability screening metric; confirm the applicable material standard, thickness, and welding procedure before making a process decision.",
            Self::Pcm => "Pcm is a weldability screening metric for low-carbon steels; confirm the applicable material standard, hydrogen control, thickness, and welding procedure before making a process decision.",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CarbonEquivalentResult {
    pub formula_id: String,
    pub expression: String,
    pub normalized_inputs: BTreeMap<String, f64>,
    pub value: f64,
    pub unit: CompositionUnit,
    pub applicability_note: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SteelCalculationError {
    EmptyComposition,
    UnknownElement {
        element: String,
    },
    MissingElement {
        element: String,
        formula: String,
    },
    ValueOutOfRange {
        element: String,
        value: f64,
        unit: CompositionUnit,
    },
}

impl Display for SteelCalculationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyComposition => formatter.write_str("composition must not be empty"),
            Self::UnknownElement { element } => write!(formatter, "unsupported element: {element}"),
            Self::MissingElement { element, formula } => {
                write!(formatter, "{formula} requires element {element}")
            }
            Self::ValueOutOfRange {
                element,
                value,
                unit,
            } => {
                write!(formatter, "{element} value {value} is invalid for {unit:?}")
            }
        }
    }
}

impl std::error::Error for SteelCalculationError {}

pub fn calculate_carbon_equivalent(
    input: &CompositionInput,
    formula: CarbonEquivalentFormula,
) -> Result<CarbonEquivalentResult, SteelCalculationError> {
    if input.values.is_empty() {
        return Err(SteelCalculationError::EmptyComposition);
    }
    let mut normalized_inputs = BTreeMap::new();
    for (element, value) in &input.values {
        if !SUPPORTED_ELEMENTS.contains(&element.as_str()) {
            return Err(SteelCalculationError::UnknownElement {
                element: element.clone(),
            });
        }
        let normalized = match input.unit {
            CompositionUnit::PercentMass if value.is_finite() && (0.0..=100.0).contains(value) => {
                *value
            }
            CompositionUnit::MassFraction if value.is_finite() && (0.0..=1.0).contains(value) => {
                value * 100.0
            }
            _ => {
                return Err(SteelCalculationError::ValueOutOfRange {
                    element: element.clone(),
                    value: *value,
                    unit: input.unit,
                })
            }
        };
        normalized_inputs.insert(element.clone(), normalized);
    }
    for element in formula.required_elements() {
        if !normalized_inputs.contains_key(*element) {
            return Err(SteelCalculationError::MissingElement {
                element: (*element).to_string(),
                formula: formula.id().to_string(),
            });
        }
    }

    let value = |element: &str| normalized_inputs[element];
    let result = match formula {
        CarbonEquivalentFormula::Iiw => {
            value("C")
                + value("Mn") / 6.0
                + (value("Cr") + value("Mo") + value("V")) / 5.0
                + (value("Ni") + value("Cu")) / 15.0
        }
        CarbonEquivalentFormula::Pcm => {
            value("C")
                + value("Si") / 30.0
                + (value("Mn") + value("Cu") + value("Cr")) / 20.0
                + value("Ni") / 60.0
                + value("Mo") / 15.0
                + value("V") / 10.0
                + 5.0 * value("B")
        }
    };

    Ok(CarbonEquivalentResult {
        formula_id: formula.id().to_string(),
        expression: formula.expression().to_string(),
        normalized_inputs,
        value: result,
        unit: CompositionUnit::PercentMass,
        applicability_note: formula.applicability_note().to_string(),
    })
}
