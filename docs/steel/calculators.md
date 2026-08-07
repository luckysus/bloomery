# Steel Calculators / 钢铁计算器

Bloomery currently exposes two deterministic carbon-equivalent calculations as a read-only built-in tool: IIW and Pcm. Both return the formula id, expression, normalized composition, result unit, value, and an applicability note.

Bloomery 当前提供两个只读确定性内置工具：IIW 和 Pcm 碳当量计算。结果包含公式 ID、表达式、归一化成分、结果单位、数值和适用性说明。

## Supported Formulas / 支持公式

| Formula ID | Expression | Use |
| --- | --- | --- |
| `carbon-equivalent.iiw.v1` | `C + Mn/6 + (Cr + Mo + V)/5 + (Ni + Cu)/15` | Weldability screening |
| `carbon-equivalent.pcm.v1` | `C + Si/30 + (Mn + Cu + Cr)/20 + Ni/60 + Mo/15 + V/10 + 5B` | Low-carbon steel weldability screening |

Composition values may be supplied as `percent_mass` or `mass_fraction`; the result is always expressed in mass percent. Common steel elements such as Fe, P, S, Al, Nb, and Ti are accepted and preserved even when a formula does not use them. Unknown element symbols and missing formula inputs are rejected.

成分可以使用质量百分数或质量分数输入，结果统一为质量百分数。即使某个公式不使用，Fe、P、S、Al、Nb、Ti 等常见钢铁元素也会被保留；未知元素符号和公式必需元素缺失会被拒绝。

## Tool Contract / 工具契约

Tool id: `steel.carbon_equivalent`

```json
{
  "formula": "iiw",
  "unit": "percent_mass",
  "composition": {
    "C": 0.20,
    "Mn": 1.00,
    "Cr": 0.25,
    "Mo": 0.05,
    "V": 0.02,
    "Ni": 0.20,
    "Cu": 0.30
  }
}
```

The calculator is a screening aid, not a substitute for the applicable material standard, product thickness, hydrogen control, welding procedure qualification, or engineering review. It does not silently select another formula when inputs are missing or invalid.

该工具仅用于筛查，不能替代材料标准、产品厚度、氢控制、焊接工艺评定或工程审核。输入缺失或非法时不会静默切换到其他公式。

