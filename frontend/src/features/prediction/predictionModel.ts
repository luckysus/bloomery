import type { SteelTrainingAlgorithm } from "../../bridge/desktop";

/** 第 35 章模型中心：可选算法族与显示名。 */
export const TRAINING_ALGORITHMS: ReadonlyArray<{
  value: SteelTrainingAlgorithm;
  label: string;
  family: string;
}> = [
  { value: "linear_regression", label: "Linear Regression", family: "线性模型" },
  { value: "elasticnet", label: "ElasticNet", family: "线性模型" },
  { value: "random_forest", label: "Random Forest", family: "树集成" },
  { value: "hist_gradient_boosting", label: "HistGradientBoosting", family: "梯度提升" },
  { value: "lightgbm", label: "LightGBM", family: "梯度提升" },
  { value: "xgboost", label: "XGBoost", family: "梯度提升" },
  { value: "svr", label: "SVR", family: "核方法" },
  { value: "mlp", label: "MLP", family: "神经网络" },
  { value: "transformer", label: "Transformer", family: "神经网络" },
];

export function algorithmLabel(value: string) {
  return TRAINING_ALGORITHMS.find((item) => item.value === value)?.label ?? value;
}

/** 第 33 章：常见钢铁性能指标的语义与单位。 */
export const STEEL_PROPERTIES: Record<string, { label: string; unit: string }> = {
  ys: { label: "屈服强度 YS", unit: "MPa" },
  yield_strength: { label: "屈服强度 YS", unit: "MPa" },
  ts: { label: "抗拉强度 TS", unit: "MPa" },
  tensile_strength: { label: "抗拉强度 TS", unit: "MPa" },
  el: { label: "延伸率 EL", unit: "%" },
  elongation: { label: "延伸率 EL", unit: "%" },
  hardness: { label: "硬度", unit: "HV" },
  hv: { label: "硬度", unit: "HV" },
  impact_toughness: { label: "冲击韧性", unit: "J" },
  impact_energy: { label: "冲击韧性", unit: "J" },
  wear_resistance: { label: "耐磨性", unit: "mg/mm²" },
  wear_rate: { label: "耐磨性", unit: "mg/mm²" },
  r_value: { label: "塑性应变比 r-value", unit: "" },
  rvalue: { label: "塑性应变比 r-value", unit: "" },
  grain_size: { label: "晶粒尺寸", unit: "μm" },
  grain_size_um: { label: "晶粒尺寸", unit: "μm" },
  microstructure_ratio: { label: "组织比例", unit: "%" },
  phase_ratio: { label: "组织比例", unit: "%" },
};

/** 按规范字段或原始列名识别钢铁性能指标；识别不到返回 null。 */
export function steelPropertyOf(column: { canonicalField?: string | null; originalName?: string; unit?: string | null }) {
  const candidates = [column.canonicalField, column.originalName].filter(
    (value): value is string => typeof value === "string" && value.trim().length > 0,
  );
  for (const candidate of candidates) {
    const key = candidate.trim().toLowerCase().replace(/[\s-]+/g, "_");
    const match = STEEL_PROPERTIES[key];
    if (match) return match;
  }
  return null;
}

/** 目标/特征列的统一显示名：优先性能指标语义，其次原始列名 + 单位。 */
export function columnDisplayName(column: { canonicalField?: string | null; originalName?: string; unit?: string | null }) {
  const property = steelPropertyOf(column);
  if (property) return property.unit ? `${property.label} (${property.unit})` : property.label;
  const unit = column.unit?.trim();
  return unit ? `${column.originalName} (${unit})` : column.originalName ?? "";
}

/** 指标可能为 null（样本不足），统一格式化为短字符串。 */
export function formatMetricValue(value: unknown, digits = 4) {
  if (typeof value !== "number" || !Number.isFinite(value)) return "-";
  return Number.isInteger(value) ? String(value) : value.toFixed(digits);
}

/** 第 37 章 Feature Importance：把绝对重要度换算成占比并降序。 */
export function featureImportanceShares(names: string[], importance: number[] | undefined) {
  if (!importance || importance.length === 0) return [];
  const total = importance.reduce((sum, value) => sum + Math.abs(value), 0);
  return names
    .map((name, index) => ({ name, share: total > 0 ? Math.abs(importance[index] ?? 0) / total : 0 }))
    .sort((left, right) => right.share - left.share);
}
