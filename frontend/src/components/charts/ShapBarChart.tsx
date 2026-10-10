import type { ShapExplanation } from "../../bridge/desktop";

/**
 * SHAP 贡献条形图（第 37 章）。
 *
 * 数值来自后端对线性模型产物的**精确 SHAP**（满足效率性：基准值 + 各贡献 = 预测值）。
 * 前端不做近似，也不把特征重要性当作 SHAP。
 */
export default function ShapBarChart({ explanation }: { explanation: ShapExplanation }) {
  const maxAbs = explanation.values.reduce((max, item) => Math.max(max, Math.abs(item.contribution)), 0);

  return (
    <div className="suna-shap">
      <div className="suna-shap-summary">
        <span>基准值 <strong>{explanation.baseValue.toFixed(3)}</strong></span>
        <span>预测值 <strong>{explanation.prediction.toFixed(3)}</strong></span>
        <span>模型 <strong>{explanation.modelType}</strong></span>
      </div>
      <ul className="suna-shap-list">
        {explanation.values.map((item) => {
          const ratio = maxAbs > 0 ? Math.abs(item.contribution) / maxAbs : 0;
          const positive = item.contribution >= 0;
          return (
            <li key={item.feature}>
              <span className="suna-shap-feature">{item.feature}</span>
              <span className="suna-shap-track">
                <i
                  className={positive ? "is-positive" : "is-negative"}
                  style={{ width: `${Math.max(1, ratio * 100)}%` }}
                />
              </span>
              <span className={`suna-shap-value ${positive ? "is-positive" : "is-negative"}`}>
                {positive ? "+" : ""}{item.contribution.toFixed(3)}
              </span>
              <span className="suna-shap-share">{(item.share * 100).toFixed(1)}%</span>
            </li>
          );
        })}
      </ul>
      <p className="suna-correlation-legend">
        精确 SHAP：各特征贡献之和加上基准值等于模型预测值。仅支持线性模型；其他模型族会明确提示不支持。
      </p>
    </div>
  );
}
