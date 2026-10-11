import math

import pytest

from suna_worker.design import (
    CANDIDATE_SAMPLES,
    DesignError,
    _design_ccd,
    _hadamard_two_level,
    _prime_square_table,
    _table_is_orthogonal,
    design_experiments,
)


def variables(*specs):
    return [{"name": name, "low": low, "high": high} for name, low, high in specs]


def existing_data(rows=40, noise=0.0):
    # 目标 y = -(x1 - 0.7)^2 + 0.5*x2（归一化域内），最优在 x1≈0.7、x2 大处。
    features = []
    targets = []
    for index in range(rows):
        a = (index % 10) / 9.0
        b = ((index // 10) % 4) / 3.0
        features.append([a, b])
        targets.append(-((a - 0.7) ** 2) + 0.5 * b + noise)
    return {
        "feature_names": ["temperature", "time"],
        "features": features,
        "targets": targets,
    }


# ---------------------------------------------------------------------------
# DOE（第 45 章）


def test_doe_covers_all_level_combinations():
    result = design_experiments(
        {
            "method": "doe",
            "variables": variables(("temperature", 800.0, 900.0), ("time", 10.0, 60.0)),
            "levels": 3,
        }
    )

    assert result["method"] == "doe"
    assert len(result["points"]) == 9
    temperatures = sorted({point["values"]["temperature"] for point in result["points"]})
    assert temperatures == [pytest.approx(800.0), pytest.approx(850.0), pytest.approx(900.0)]
    assert "全因子设计" in result["notes"][0]


def test_doe_rejects_designs_beyond_the_point_budget():
    with pytest.raises(DesignError) as excinfo:
        design_experiments(
            {
                "method": "doe",
                "variables": variables(*[(f"x{i}", 0.0, 1.0) for i in range(5)]),
                "levels": 4,
            }
        )
    assert excinfo.value.code == "design_too_large"


# ---------------------------------------------------------------------------
# 正交表（第 45 章）


def test_hadamard_two_level_tables_are_strength_two_orthogonal():
    for exponent in (2, 3, 4):
        table = _hadamard_two_level(exponent)
        levels = 2
        assert len(table) == 2**exponent
        assert len(table[0]) == 2**exponent - 1
        assert _table_is_orthogonal(table, levels)


def test_prime_square_tables_match_standard_taguchi_arrays():
    # 标准 L9(3^4)（0 起始水平编码）。
    assert _prime_square_table(3) == [
        [0, 0, 0, 0],
        [0, 1, 1, 1],
        [0, 2, 2, 2],
        [1, 0, 1, 2],
        [1, 1, 2, 0],
        [1, 2, 0, 1],
        [2, 0, 2, 1],
        [2, 1, 0, 2],
        [2, 2, 1, 0],
    ]
    assert _table_is_orthogonal(_prime_square_table(5), 5)


def test_orthogonal_selects_the_smallest_covering_table():
    result = design_experiments(
        {
            "method": "orthogonal",
            "variables": variables(("a", 0.0, 1.0), ("b", 0.0, 1.0)),
        }
    )
    # 2 因素 2 水平：L4 是最小覆盖表。
    assert len(result["points"]) == 4
    assert "L4" in result["notes"][0]


def test_orthogonal_maps_levels_across_the_variable_range():
    result = design_experiments(
        {
            "method": "orthogonal",
            "variables": variables(("temperature", 800.0, 900.0)),
            "levels": 3,
        }
    )
    values = {round(point["values"]["temperature"], 6) for point in result["points"]}
    assert values == {800.0, 850.0, 900.0}


def test_orthogonal_reports_missing_tables_honestly():
    payload = {
        "method": "orthogonal",
        "variables": variables(*[(f"x{i}", 0.0, 1.0) for i in range(5)]),
        "levels": 3,
    }
    with pytest.raises(DesignError) as excinfo:
        design_experiments(payload)
    assert excinfo.value.code == "no_orthogonal_table"
    assert "L9" in str(excinfo.value) or "built-in coverage" in str(excinfo.value)


# ---------------------------------------------------------------------------
# CCD 响应面（第 45 章）


def test_ccd_structure_is_corners_axial_and_center():
    points, notes = _design_ccd(
        variables(("temperature", 800.0, 900.0), ("time", 10.0, 60.0))
    )

    # 4 角点 + 4 轴点 + 4 中心点。
    assert len(points) == 12
    alpha = (2.0**2) ** 0.25
    corners = [point["values"] for point in points[:4]]
    corner_temperature = sorted({round(value["temperature"], 9) for value in corners})
    expected_temperature = sorted(
        {round(850.0 + sign * 50.0 / alpha, 9) for sign in (1.0, -1.0)}
    )
    assert corner_temperature == expected_temperature
    corner_time = sorted({round(value["time"], 9) for value in corners})
    expected_time = sorted({round(35.0 + sign * 25.0 / alpha, 9) for sign in (1.0, -1.0)})
    assert corner_time == expected_time
    axial = [point["values"] for point in points[4:8]]
    assert any(math.isclose(item["temperature"], 900.0) for item in axial)
    assert any(math.isclose(item["temperature"], 800.0) for item in axial)
    centers = points[8:]
    assert all(
        math.isclose(item["values"]["temperature"], 850.0)
        for item in centers
    )
    assert "可旋转" in notes[1]


def test_ccd_requires_two_variables():
    with pytest.raises(DesignError) as excinfo:
        design_experiments(
            {"method": "ccd", "variables": variables(("temperature", 800.0, 900.0))}
        )
    assert excinfo.value.code == "invalid_payload"


# ---------------------------------------------------------------------------
# 贝叶斯优化与主动学习（第 44/46 章）


def test_bayesian_recommends_near_the_optimum_and_is_deterministic():
    payload = {
        "method": "bayesian",
        "variables": variables(("temperature", 0.0, 1.0), ("time", 0.0, 1.0)),
        "existing": existing_data(),
        "count": 3,
        "seed": 11,
    }
    result = design_experiments(payload)
    second = design_experiments(payload)

    assert result["method"] == "bayesian"
    assert len(result["points"]) == 3
    assert result["points"] == second["points"]
    # 目标在 x1≈0.7、x2 大处最优；推荐点必须靠近该区域。
    best = result["points"][0]
    assert abs(best["values"]["temperature"] - 0.7) < 0.2
    assert best["values"]["time"] > 0.6
    assert best["expected_improvement"] > 0
    assert best["predicted_std"] >= 0
    assert "期望改进" in result["notes"][0]


def test_bayesian_maximize_and_minimize_point_in_opposite_directions():
    base = {
        "variables": variables(("temperature", 0.0, 1.0), ("time", 0.0, 1.0)),
        "existing": existing_data(),
        "count": 2,
        "seed": 11,
    }
    maximize = design_experiments({**base, "method": "bayesian", "direction": "maximize"})
    minimize = design_experiments({**base, "method": "bayesian", "direction": "minimize"})
    # 目标在 temperature≈0.7 最大，0 附近较小。
    assert maximize["points"][0]["values"]["temperature"] > 0.5
    assert minimize["points"][0]["values"]["temperature"] < 0.4


def test_active_learning_prefers_uncovered_regions():
    # 已有数据集中在 x1∈[0, 0.5]；主动学习应推荐 x1 大（覆盖缺口）的组合。
    features = [[index / 40 * 0.5, (index % 4) / 3.0] for index in range(40)]
    targets = [0.1 * index for index in range(40)]
    result = design_experiments(
        {
            "method": "active_learning",
            "variables": variables(("temperature", 0.0, 1.0), ("time", 0.0, 1.0)),
            "existing": {"feature_names": ["temperature", "time"], "features": features, "targets": targets},
            "count": 3,
            "seed": 5,
        }
    )

    assert result["method"] == "active_learning"
    assert len(result["points"]) == 3
    top = result["points"][0]
    assert top["predicted_std"] >= result["points"][-1]["predicted_std"]
    assert top["coverage_gap"] > 0.4
    assert "主动学习" in result["notes"][0]


# ---------------------------------------------------------------------------
# 校验与错误路径


def test_uncertainty_methods_require_existing_data():
    for method in ("bayesian", "active_learning"):
        with pytest.raises(DesignError) as excinfo:
            design_experiments(
                {
                    "method": method,
                    "variables": variables(("temperature", 0.0, 1.0)),
                }
            )
        assert excinfo.value.code == "missing_existing"


def test_existing_rows_must_be_enough_and_names_must_match():
    payload = {
        "method": "bayesian",
        "variables": variables(("temperature", 0.0, 1.0), ("time", 0.0, 1.0)),
        "existing": existing_data(rows=4),
    }
    with pytest.raises(DesignError) as excinfo:
        design_experiments(payload)
    assert excinfo.value.code == "insufficient_existing"

    mismatched = existing_data()
    mismatched["feature_names"] = ["temperature", "pressure"]
    with pytest.raises(DesignError) as excinfo:
        design_experiments({**payload, "existing": mismatched})
    assert excinfo.value.code == "existing_mismatch"


@pytest.mark.parametrize(
    "overrides,code",
    [
        ({"method": "lstm"}, "unsupported_method"),
        ({"variables": []}, "invalid_payload"),
        ({"variables": [{"name": "a", "low": 2.0, "high": 1.0}]}, "invalid_bounds"),
        ({"variables": [{"name": "a", "low": 0.0}, {"name": "a", "low": 0.0, "high": 1.0}]}, "invalid_payload"),
        ({"direction": "sideways", "method": "doe", "variables": variables(("a", 0.0, 1.0))}, "invalid_payload"),
        ({"count": 0, "method": "doe", "variables": variables(("a", 0.0, 1.0))}, "invalid_payload"),
        ({"levels": 6, "method": "doe", "variables": variables(("a", 0.0, 1.0))}, "invalid_payload"),
    ],
)
def test_invalid_payloads_are_rejected_with_typed_codes(overrides, code):
    base = {"method": "doe", "variables": variables(("temperature", 800.0, 900.0))}
    payload = {**base, **overrides}
    if payload["method"] in ("bayesian", "active_learning"):
        payload["existing"] = existing_data()
    with pytest.raises(DesignError) as excinfo:
        design_experiments(payload)
    assert excinfo.value.code == code


def test_progress_reports_design_stages():
    stages = []
    design_experiments(
        {
            "method": "bayesian",
            "variables": variables(("temperature", 0.0, 1.0), ("time", 0.0, 1.0)),
            "existing": existing_data(),
        },
        report=lambda stage, progress: stages.append((stage, progress)),
    )
    names = [stage for stage, _ in stages]
    assert names[0] == "validated" and names[-1] == "validated"
    assert "modelling" in names
    values = [progress for _, progress in stages]
    assert values == sorted(values) and values[-1] <= 99


def test_candidate_budget_is_respected():
    assert CANDIDATE_SAMPLES == 2048
