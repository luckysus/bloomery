# Steel Data Model / 钢铁数据模型

The steel package is a vocabulary and mapping layer. User data remains local to the selected Bloomery workspace and is never copied into the package.

钢铁包只负责术语和映射。用户数据属于当前 Bloomery 工作区，保留在本地，不会写入领域包。

## Canonical Identifiers / 规范字段

| Field | Meaning | Unit or rule |
| --- | --- | --- |
| `heat_id` | Traceable melt or heat identifier | Preserve source text; do not coerce leading zeroes |
| `steel_grade` | Product or melt grade label | Preserve original label and normalized alias separately |
| `cast_time` | Casting timestamp | Store the source timezone or mark it unknown |
| `temperature_c` | Temperature | Celsius after explicit conversion |
| `carbon_pct` | Carbon mass fraction | Percent by mass, not a fraction from 0 to 1 |
| `silicon_pct` | Silicon mass fraction | Percent by mass |
| `manganese_pct` | Manganese mass fraction | Percent by mass |
| `phosphorus_pct` | Phosphorus mass fraction | Percent by mass |
| `sulfur_pct` | Sulfur mass fraction | Percent by mass |

## Mapping Rules / 映射规则

1. Preserve every original column name and the source file hash.
2. Preview inferred mappings and invalid/missing values before activation.
3. Require `heat_id` for traceability-oriented analysis; reject duplicate canonical mappings unless the user resolves them.
4. Perform unit conversion only when the source unit is explicit or the user confirms it.
5. Keep missing, invalid, and converted values in the import quality report.

1. 保留原始列名和源文件摘要。
2. 激活数据集前先预览推断映射以及缺失/非法值。
3. 面向炉次追溯的分析要求 `heat_id`；规范字段重复映射时必须由用户确认。
4. 只有源单位明确或用户确认后才进行单位转换。
5. 导入质量报告必须保留缺失、非法和已转换数据统计。

The package mapping file is a preset, not a schema that silently changes user data. Every import creates a new versioned source record and can be audited or removed through the local data-management workflow.
