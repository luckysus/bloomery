//! 模型解释：线性模型的精确 SHAP（第 37 章）。
//!
//! 线性回归的 Shapley 值有闭式解，因此这里给出的是**精确 SHAP**，不是采样近似：
//!
//! ```text
//! ŷ = intercept + Σ wᵢ · (xᵢ − meanᵢ) / scaleᵢ
//! φᵢ = wᵢ · (xᵢ − meanᵢ) / scaleᵢ        （特征 i 的贡献）
//! φ₀ = intercept                          （基准值）
//! ```
//!
//! 满足效率性：`φ₀ + Σ φᵢ = ŷ`。单元测试会断言这一点。
//!
//! 非线性的模型族（随机森林、XGBoost、ONNX）没有闭式解，本模块**明确拒绝**，
//! 不会用特征重要性冒充 SHAP。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq)]
pub struct LinearModel {
    pub feature_names: Vec<String>,
    pub means: Vec<f64>,
    pub scales: Vec<f64>,
    pub coefficients: Vec<f64>,
    pub intercept: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShapValue {
    pub feature: String,
    pub value: f64,
    /// 该特征对预测值的贡献（可正可负）。
    pub contribution: f64,
    /// 贡献绝对值占比，用于排序与条形图；总和为 1（贡献全为 0 时为 0）。
    pub share: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShapExplanation {
    pub model_type: String,
    pub base_value: f64,
    pub prediction: f64,
    pub values: Vec<ShapValue>,
}

fn number(value: &serde_json::Value, key: &str) -> Result<f64, String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_f64)
        .filter(|item| item.is_finite())
        .ok_or_else(|| format!("artifact field `{key}` is missing or not a finite number"))
}

fn number_array(value: &serde_json::Value, key: &str) -> Result<Vec<f64>, String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("artifact field `{key}` is missing or not an array"))?
        .iter()
        .map(|item| {
            item.as_f64()
                .filter(|number| number.is_finite())
                .ok_or_else(|| format!("artifact field `{key}` contains a non-finite value"))
        })
        .collect()
}

fn string_array(value: &serde_json::Value, key: &str) -> Result<Vec<String>, String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("artifact field `{key}` is missing or not an array"))?
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("artifact field `{key}` contains a non-string value"))
        })
        .collect()
}

/// 从线性模型产物中解析出 SHAP 计算所需字段。
pub fn parse_linear_artifact(artifact: &serde_json::Value) -> Result<LinearModel, String> {
    let model_type = artifact
        .get("model_type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if model_type != "linear_regression" {
        return Err(format!(
            "exact SHAP is only available for linear_regression artifacts; `{model_type}` needs a sampling estimator"
        ));
    }

    let feature_names = string_array(artifact, "feature_names")?;
    let coefficients = number_array(artifact, "coefficients")?;
    let intercept = number(artifact, "intercept")?;
    let preprocessing = artifact
        .get("preprocessing")
        .ok_or_else(|| "artifact field `preprocessing` is missing".to_string())?;
    let means = number_array(preprocessing, "means")?;
    let scales = number_array(preprocessing, "scales")?;

    let count = feature_names.len();
    if count == 0 {
        return Err("artifact has no features".to_string());
    }
    if coefficients.len() != count || means.len() != count || scales.len() != count {
        return Err("artifact feature metadata lengths do not match".to_string());
    }
    if scales.iter().any(|scale| scale.abs() < 1e-12) {
        return Err("artifact contains a zero scale; the model cannot be explained".to_string());
    }

    Ok(LinearModel { feature_names, means, scales, coefficients, intercept })
}

/// 计算单个样本的精确 SHAP 值。
pub fn explain_linear(model: &LinearModel, features: &[f64]) -> Result<ShapExplanation, String> {
    if features.len() != model.feature_names.len() {
        return Err(format!(
            "expected {} feature values but received {}",
            model.feature_names.len(),
            features.len()
        ));
    }
    if features.iter().any(|value| !value.is_finite()) {
        return Err("feature values must be finite numbers".to_string());
    }

    let contributions: Vec<f64> = model
        .coefficients
        .iter()
        .enumerate()
        .map(|(index, coefficient)| coefficient * (features[index] - model.means[index]) / model.scales[index])
        .collect();
    let total: f64 = contributions.iter().map(|value| value.abs()).sum();
    let prediction = model.intercept + contributions.iter().sum::<f64>();

    let mut values: Vec<ShapValue> = model
        .feature_names
        .iter()
        .enumerate()
        .map(|(index, name)| ShapValue {
            feature: name.clone(),
            value: features[index],
            contribution: contributions[index],
            share: if total > 1e-12 { contributions[index].abs() / total } else { 0.0 },
        })
        .collect();
    // 按贡献绝对值降序，前端直接按顺序渲染即可。
    values.sort_by(|left, right| {
        right
            .contribution
            .abs()
            .partial_cmp(&left.contribution.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(ShapExplanation {
        model_type: "linear_regression".to_string(),
        base_value: model.intercept,
        prediction,
        values,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn artifact() -> serde_json::Value {
        json!({
            "model_type": "linear_regression",
            "feature_names": ["C", "Mn"],
            "coefficients": [30.0, 10.0],
            "intercept": 200.0,
            "preprocessing": { "means": [0.2, 1.0], "scales": [0.1, 0.5] }
        })
    }

    #[test]
    fn explanations_satisfy_the_efficiency_property() {
        let model = parse_linear_artifact(&artifact()).expect("parse artifact");
        let explanation = explain_linear(&model, &[0.3, 1.5]).expect("explain");

        // 效率性：基准值 + 各特征贡献 = 模型预测值。
        let sum = explanation.base_value + explanation.values.iter().map(|item| item.contribution).sum::<f64>();
        assert!((sum - explanation.prediction).abs() < 1e-9);
        // C: 30 * (0.3-0.2)/0.1 = 30；Mn: 10 * (1.5-1.0)/0.5 = 10
        assert!((explanation.prediction - 240.0).abs() < 1e-9);
        assert_eq!(explanation.values[0].feature, "C");
        assert!((explanation.values[0].contribution - 30.0).abs() < 1e-9);
    }

    #[test]
    fn shares_sum_to_one_and_follow_absolute_contribution() {
        let model = parse_linear_artifact(&artifact()).expect("parse artifact");
        let explanation = explain_linear(&model, &[0.3, 1.5]).expect("explain");
        let total: f64 = explanation.values.iter().map(|item| item.share).sum();
        assert!((total - 1.0).abs() < 1e-9);
        assert!(explanation.values[0].share > explanation.values[1].share);
    }

    #[test]
    fn rejects_non_linear_artifacts_instead_of_faking_shap() {
        let mut value = artifact();
        value["model_type"] = json!("random_forest");
        assert!(parse_linear_artifact(&value).is_err());
    }

    #[test]
    fn rejects_mismatched_or_missing_fields() {
        let mut value = artifact();
        value["coefficients"] = json!([1.0]);
        assert!(parse_linear_artifact(&value).is_err());

        let model = parse_linear_artifact(&artifact()).expect("parse artifact");
        assert!(explain_linear(&model, &[1.0]).is_err());
        assert!(explain_linear(&model, &[f64::NAN, 1.0]).is_err());
    }
}
