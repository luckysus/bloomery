"""实验设计算法（第 44–46 章）。

支持五种方法，全部是**真实算法**，不是提示词包装：

- ``doe``：全因子设计（2–5 水平，组合数上限 512）；
- ``orthogonal``：正交表——2 水平用 Sylvester Hadamard 构造的 L4/L8/L16/L32
  （程序化生成，任意两列水平组合均衡），3 水平 L9 与 5 水平 L25 用
  L_{p²}(p^{p+1}) 循环构造（与标准 Taguchi 表逐行一致）；
- ``ccd``：中心复合响应面设计（可旋转 α，给定范围作为轴点边界）；
- ``bayesian``：高斯过程代理 + 期望改进（EI），基于已有实验推荐下一组
  最有价值的实验（第 46 章）；
- ``active_learning``：高斯过程预测方差（探索）+ 覆盖缺口，推荐最不确定
  的实验组合。

不确定性类方法需要已有实验数据（≥5 行），变量空间统一归一化到 [0,1]³。
"""

from __future__ import annotations

import itertools
import math
from collections.abc import Callable, Mapping, Sequence
from typing import Any

SUPPORTED_METHODS = {"doe", "orthogonal", "ccd", "bayesian", "active_learning"}
UNCERTAINTY_METHODS = {"bayesian", "active_learning"}

MAX_VARIABLES = 12
MAX_DESIGN_POINTS = 512
MAX_EXISTING_ROWS = 5000
MIN_EXISTING_ROWS = 5
CANDIDATE_SAMPLES = 2048
DEFAULT_COUNT = 4
DEFAULT_CENTER_POINTS = 4
MAX_VARIABLES_CCD = 6

class DesignError(ValueError):
    def __init__(self, code: str, message: str, details: dict[str, Any] | None = None) -> None:
        super().__init__(message)
        self.code = code
        self.details = details


def design_experiments(
    payload: Mapping[str, Any],
    report: Callable[[str, int], None] | None = None,
    is_cancelled: Callable[[], bool] | None = None,
) -> dict[str, Any]:
    """Generate a designed experiment set and per-point explanations."""

    def _report(stage: str, progress: int) -> None:
        if report is not None:
            report(stage, progress)

    if not isinstance(payload, Mapping):
        raise DesignError("invalid_payload", "design payload must be an object")
    variables = _variables(payload.get("variables"))
    names = [variable["name"] for variable in variables]
    method = payload.get("method")
    if method not in SUPPORTED_METHODS:
        raise DesignError(
            "unsupported_method",
            f"method must be one of {sorted(SUPPORTED_METHODS)}",
        )
    direction = _direction(payload.get("direction"))
    seed = _integer(payload.get("seed", 0), "seed", 0, 2**31 - 1)
    levels = _integer(payload.get("levels", 2), "levels", 2, 5)
    count = _integer(payload.get("count", DEFAULT_COUNT), "count", 1, 20)
    _report("validated", 10)

    if method == "doe":
        points, notes = _design_doe(variables, levels)
    elif method == "orthogonal":
        points, notes = _design_orthogonal(variables, levels)
    elif method == "ccd":
        points, notes = _design_ccd(variables)
    else:
        existing = _existing(payload.get("existing"), names)
        if method == "bayesian":
            points, notes = _design_bayesian(
                variables, existing, direction, count, seed, _report, is_cancelled
            )
        else:
            points, notes = _design_active_learning(
                variables, existing, count, seed, _report, is_cancelled
            )
    _report("validated", 99)

    return {
        "method": method,
        "variables": names,
        "direction": direction,
        "points": points,
        "notes": notes,
    }


# ---------------------------------------------------------------------------
# 输入解析


def _variables(value: Any) -> list[dict[str, Any]]:
    if not isinstance(value, list) or not value:
        raise DesignError("invalid_payload", "variables must be a non-empty list")
    if len(value) > MAX_VARIABLES:
        raise DesignError(
            "invalid_payload", f"variables supports at most {MAX_VARIABLES} factors"
        )
    variables: list[dict[str, Any]] = []
    names: list[str] = []
    for index, item in enumerate(value):
        if not isinstance(item, Mapping):
            raise DesignError("invalid_payload", f"variables[{index}] must be an object")
        name = item.get("name")
        if not isinstance(name, str) or not name.strip():
            raise DesignError("invalid_payload", f"variables[{index}].name is required")
        if name in names:
            raise DesignError("invalid_payload", f"duplicate variable name: {name!r}")
        low = _finite(item.get("low"), f"variables[{index}].low")
        high = _finite(item.get("high"), f"variables[{index}].high")
        if low > high:
            raise DesignError("invalid_bounds", f"variables[{index}] range is inverted")
        names.append(name)
        variables.append({"name": name, "low": low, "high": high})
    return variables


def _direction(value: Any) -> str:
    if value is None:
        return "maximize"
    if value not in {"minimize", "maximize"}:
        raise DesignError("invalid_payload", "direction must be minimize or maximize")
    return str(value)


def _integer(value: Any, label: str, minimum: int, maximum: int) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise DesignError("invalid_payload", f"{label} must be an integer")
    if not minimum <= value <= maximum:
        raise DesignError("invalid_payload", f"{label} must be between {minimum} and {maximum}")
    return value


def _finite(value: Any, label: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise DesignError("invalid_payload", f"{label} must be a number")
    number = float(value)
    if not math.isfinite(number):
        raise DesignError("invalid_payload", f"{label} must be finite")
    return number


def _existing(value: Any, names: Sequence[str]) -> dict[str, Any]:
    if not isinstance(value, Mapping):
        raise DesignError(
            "missing_existing",
            "bayesian and active_learning require existing experiment data",
        )
    feature_names = value.get("feature_names")
    features = value.get("features")
    targets = value.get("targets")
    if (
        not isinstance(feature_names, list)
        or list(feature_names) != list(names)
    ):
        raise DesignError(
            "existing_mismatch",
            "existing feature names must match the design variables in order",
        )
    if not isinstance(features, list) or not isinstance(targets, list):
        raise DesignError("invalid_payload", "existing features and targets are required")
    if len(targets) != len(features):
        raise DesignError("invalid_payload", "targets must align with feature rows")
    if len(features) < MIN_EXISTING_ROWS:
        raise DesignError(
            "insufficient_existing",
            f"bayesian and active_learning require at least {MIN_EXISTING_ROWS} existing rows",
        )
    if len(features) > MAX_EXISTING_ROWS:
        import random

        keep = sorted(random.Random(0).sample(range(len(features)), MAX_EXISTING_ROWS))
        features = [features[index] for index in keep]
        targets = [targets[index] for index in keep]
    rows: list[list[float]] = []
    for row_index, row in enumerate(features):
        if not isinstance(row, list) or len(row) != len(names):
            raise DesignError(
                "invalid_payload", f"existing.features[{row_index}] must have {len(names)} columns"
            )
        rows.append([_finite(item, f"existing.features[{row_index}][{column}]") for column, item in enumerate(row)])
    numeric_targets = [
        _finite(item, f"existing.targets[{index}]") for index, item in enumerate(targets)
    ]
    return {"features": rows, "targets": numeric_targets}


# ---------------------------------------------------------------------------
# 值域映射与正交表构造


def _map_level(variable: Mapping[str, Any], level: int, levels: int) -> float:
    low, high = float(variable["low"]), float(variable["high"])
    if levels <= 1 or high <= low:
        return low
    return low + (high - low) * level / (levels - 1)


def _coded_to_values(
    variables: Sequence[Mapping[str, Any]], coded: Sequence[float]
) -> dict[str, float]:
    """把 [-1, 1] 编码坐标映射回真实值：mid + coded * half。"""
    values: dict[str, float] = {}
    for variable, coordinate in zip(variables, coded):
        low, high = float(variable["low"]), float(variable["high"])
        mid = (low + high) / 2.0
        half = (high - low) / 2.0
        values[str(variable["name"])] = mid + coordinate * half
    return values


def _hadamard_two_level(exponent: int) -> list[list[int]]:
    """Sylvester Hadamard → OA(2^m, 2^m − 1, 2, 2)：去掉全 1 列后的字符函数。"""
    size = 1 << exponent
    return [
        [bin(r & c).count("1") % 2 for c in range(1, size)]
        for r in range(size)
    ]


def _prime_square_table(p: int) -> list[list[int]]:
    """L_{p²}(p^{p+1})：列 0 为行组，列 c 为循环移位（奇素数 p）。"""
    return [
        [r // p] + [((r % p) + (c - 1) * (r // p)) % p for c in range(1, p + 1)]
        for r in range(p * p)
    ]


# 正交表族：(水平数, 每张表 (试验次数, 表))。2 水平用 Hadamard（L4..L32），
# 3/5 水平用 L_{p²}(p^{p+1}) 循环构造；模块导入时构建一次。
def _orthogonal_families() -> dict[int, list[tuple[int, list[list[int]]]]]:
    return {
        2: [(4, _hadamard_two_level(2)), (8, _hadamard_two_level(3)),
            (16, _hadamard_two_level(4)), (32, _hadamard_two_level(5))],
        3: [(9, _prime_square_table(3))],
        5: [(25, _prime_square_table(5))],
    }


ORTHOGONAL_FAMILIES = _orthogonal_families()


def _table_is_orthogonal(table: Sequence[Sequence[int]], levels: int) -> bool:
    """强度 2 校验：任两列的水平组合各出现 行数/水平数² 次。"""
    runs = len(table)
    expected = runs / (levels * levels)
    for left in range(len(table[0])):
        for right in range(left + 1, len(table[0])):
            counts: dict[tuple[int, int], int] = {}
            for row in table:
                pair = (row[left], row[right])
                counts[pair] = counts.get(pair, 0) + 1
            if len(counts) != levels * levels or any(
                count != expected for count in counts.values()
            ):
                return False
    return True


# ---------------------------------------------------------------------------
# 设计引擎


def _design_doe(
    variables: Sequence[Mapping[str, Any]], levels: int
) -> tuple[list[dict[str, Any]], list[str]]:
    count_factors = len(variables)
    total = levels**count_factors
    if total > MAX_DESIGN_POINTS:
        raise DesignError(
            "design_too_large",
            f"full factorial needs {total} points; limit is {MAX_DESIGN_POINTS}. "
            "Reduce levels or variables, or use orthogonal/ccd.",
        )
    points = []
    for combination in itertools.product(range(levels), repeat=count_factors):
        values = {
            variable["name"]: _map_level(variable, level, levels)
            for variable, level in zip(variables, combination)
        }
        points.append({"values": values})
    notes = [
        f"全因子设计：{levels} 水平 × {count_factors} 变量 = {total} 组，完整覆盖所有水平组合。",
    ]
    return points, notes


def _design_orthogonal(
    variables: Sequence[Mapping[str, Any]], levels: int
) -> tuple[list[dict[str, Any]], list[str]]:
    count_factors = len(variables)
    family = ORTHOGONAL_FAMILIES.get(levels, [])
    chosen = next(((runs, table) for runs, table in family if len(table[0]) >= count_factors), None)
    if chosen is None:
        supported = {level: max(len(table[0]) for _runs, table in tables) for level, tables in ORTHOGONAL_FAMILIES.items()}
        raise DesignError(
            "no_orthogonal_table",
            f"no built-in orthogonal table covers {count_factors} factors at {levels} levels; "
            f"built-in coverage: {supported}. Use doe or ccd instead.",
        )
    runs, table = chosen
    points = []
    for row in table:
        values = {
            variable["name"]: _map_level(variable, level, levels)
            for variable, level in zip(variables, row[:count_factors])
        }
        points.append({"values": values})
    tag = f"L{runs}({levels}^{len(table[0])})"
    notes = [
        f"选用 {tag} 正交表，{runs} 次试验均衡覆盖 {count_factors} 个因素的 {levels} 个水平；"
        "任两列的水平组合出现次数相同（强度 2）。",
    ]
    if count_factors < len(table[0]):
        notes.append(f"该表共 {len(table[0])} 列，实际使用前 {count_factors} 列。")
    return points, notes


def _design_ccd(variables: Sequence[Mapping[str, Any]]) -> tuple[list[dict[str, Any]], list[str]]:
    count_factors = len(variables)
    if count_factors < 2:
        raise DesignError("invalid_payload", "ccd requires at least 2 variables")
    if count_factors > MAX_VARIABLES_CCD:
        raise DesignError(
            "invalid_payload",
            f"ccd supports at most {MAX_VARIABLES_CCD} variables; use orthogonal or bayesian instead",
        )
    alpha = (2.0**count_factors) ** 0.25
    corner = 1.0 / alpha
    points: list[dict[str, Any]] = []
    for signs in itertools.product([-corner, corner], repeat=count_factors):
        points.append({"values": _coded_to_values(variables, signs)})
    for axis in range(count_factors):
        for sign in (-1.0, 1.0):
            coded = [0.0] * count_factors
            coded[axis] = sign
            points.append({"values": _coded_to_values(variables, coded)})
    for _ in range(DEFAULT_CENTER_POINTS):
        points.append({"values": _coded_to_values(variables, [0.0] * count_factors)})
    total = 2**count_factors + 2 * count_factors + DEFAULT_CENTER_POINTS
    notes = [
        "中心复合设计（CCD）："
        f"{2**count_factors} 个因子点 + {2 * count_factors} 个轴点 + {DEFAULT_CENTER_POINTS} 个中心点，"
        f"共 {total} 组。",
        f"可旋转 α={alpha:.3f}；给定范围作为轴点（总设计域）边界，因子点位于范围的 1/α≈{1.0 / alpha:.3f} 比例处，"
        "可拟合二阶响应面。",
    ]
    return points, notes


def _fit_gp(
    variables: Sequence[Mapping[str, Any]],
    existing: Mapping[str, Any],
    seed: int,
) -> tuple[Any, Any, Any]:
    import numpy as np
    from sklearn.gaussian_process import GaussianProcessRegressor
    from sklearn.gaussian_process.kernels import RBF, WhiteKernel
    from sklearn.gaussian_process.kernels import ConstantKernel as C

    x_existing = _normalise_matrix(existing["features"], variables)
    y_existing = np.asarray(existing["targets"], dtype=np.float64)
    kernel = (
        C(1.0, (1e-2, 1e2))
        * RBF(length_scale=0.3, length_scale_bounds=(0.05, 2.0))
        + WhiteKernel(noise_level=1e-3, noise_level_bounds=(1e-6, 1e-1))
    )
    gp = GaussianProcessRegressor(
        kernel=kernel,
        normalize_y=True,
        random_state=seed,
        n_restarts_optimizer=2,
    )
    gp.fit(x_existing, y_existing)
    return gp, x_existing, y_existing


def _normalise_matrix(
    features: Sequence[Sequence[float]], variables: Sequence[Mapping[str, Any]]
) -> Any:
    import numpy as np

    rows = []
    for row in features:
        normalised = []
        for value, variable in zip(row, variables):
            low, high = float(variable["low"]), float(variable["high"])
            span = high - low
            normalised.append(0.0 if span <= 0 else (value - low) / span)
        rows.append(normalised)
    return np.asarray(rows, dtype=np.float64)


def _candidate_matrix(count_factors: int, seed: int) -> Any:
    import numpy as np

    rng = np.random.default_rng(seed)
    return rng.random((CANDIDATE_SAMPLES, count_factors))


def _design_bayesian(
    variables: Sequence[Mapping[str, Any]],
    existing: Mapping[str, Any],
    direction: str,
    count: int,
    seed: int,
    report: Callable[[str, int], None],
    is_cancelled: Callable[[], bool] | None,
) -> tuple[list[dict[str, Any]], list[str]]:
    import numpy as np

    gp, x_existing, y_existing = _fit_gp(variables, existing, seed)
    _check_cancel(is_cancelled)
    report("modelling", 40)
    candidates = _candidate_matrix(len(variables), seed)
    mean, std = gp.predict(candidates, return_std=True)
    _check_cancel(is_cancelled)
    report("modelling", 80)

    best = float(np.max(y_existing)) if direction == "maximize" else float(np.min(y_existing))
    improvement = (mean - best) if direction == "maximize" else (best - mean)
    sigma = np.maximum(std, 1e-12)
    z = improvement / sigma
    phi = np.exp(-0.5 * z * z) / math.sqrt(2.0 * math.pi)
    erf = np.vectorize(math.erf)
    cdf = 0.5 * (1.0 + erf(z / math.sqrt(2.0)))
    expected_improvement = improvement * cdf + sigma * phi
    order = np.argsort(-expected_improvement)[:count]

    points = []
    for index in order:
        points.append(
            {
                "values": _denormalise_point(variables, candidates[index]),
                "expected_improvement": float(expected_improvement[index]),
                "predicted_mean": float(mean[index]),
                "predicted_std": float(std[index]),
            }
        )
    notes = [
        f"高斯过程代理 + 期望改进（EI）：已有 {len(y_existing)} 组实验，当前最优目标值 {best:.4g}；"
        f"在 {CANDIDATE_SAMPLES} 个候选点中选出 EI 最高的 {count} 组。",
        "EI 同时考虑预测提升与模型不确定性——数据稀疏的区域自然获得更高的探索权重。",
    ]
    return points, notes


def _design_active_learning(
    variables: Sequence[Mapping[str, Any]],
    existing: Mapping[str, Any],
    count: int,
    seed: int,
    report: Callable[[str, int], None],
    is_cancelled: Callable[[], bool] | None,
) -> tuple[list[dict[str, Any]], list[str]]:
    import numpy as np

    gp, x_existing, _y_existing = _fit_gp(variables, existing, seed)
    _check_cancel(is_cancelled)
    report("modelling", 40)
    candidates = _candidate_matrix(len(variables), seed)
    _mean, std = gp.predict(candidates, return_std=True)
    _check_cancel(is_cancelled)
    report("modelling", 80)

    # 主排序按预测方差（探索）；对头部候选补充与已有实验的最小距离（覆盖缺口）。
    order = np.argsort(-std)[: max(count * 8, 64)]
    gaps: dict[int, float] = {}
    for index in order:
        point = candidates[index]
        gaps[int(index)] = float(
            np.min(np.sqrt(((x_existing - point) ** 2).sum(axis=1)))
        )
    ranked = sorted(order, key=lambda index: (-float(std[index]), -gaps[int(index)]))[:count]

    points = []
    for index in ranked:
        points.append(
            {
                "values": _denormalise_point(variables, candidates[index]),
                "predicted_std": float(std[index]),
                "coverage_gap": gaps[int(index)],
            }
        )
    notes = [
        "主动学习：优先推荐模型预测方差（predicted_std）最大的组合，用最少的实验降低模型不确定性；"
        "coverage_gap 为与已有实验的最小归一化距离，越大表示该区域覆盖越少。",
        f"已有 {len(x_existing)} 组实验；在 {CANDIDATE_SAMPLES} 个候选点中评估不确定性后选出 {count} 组。",
    ]
    return points, notes


def _denormalise_point(
    variables: Sequence[Mapping[str, Any]], normalised: Sequence[float]
) -> dict[str, float]:
    values: dict[str, float] = {}
    for variable, coordinate in zip(variables, normalised):
        low, high = float(variable["low"]), float(variable["high"])
        span = high - low
        values[str(variable["name"])] = low if span <= 0 else low + span * float(coordinate)
    return values


def _check_cancel(is_cancelled: Callable[[], bool] | None) -> None:
    if is_cancelled is not None and is_cancelled():
        raise DesignError("design_cancelled", "experiment design was cancelled")
