import math

import pytest

from bloomery_worker.training import predict_linear_regression, train_linear_regression


def test_training_is_reproducible_and_group_split_does_not_leak() -> None:
    payload = {
        "features": [[0], [1], [2], [3], [4], [5], [6], [7]],
        "targets": [1, 3, 5, 7, 9, 11, 13, 15],
        "feature_names": ["temperature"],
        "groups": ["heat-a", "heat-a", "heat-b", "heat-b", "heat-c", "heat-c", "heat-d", "heat-d"],
        "split_policy": {"kind": "group", "validation_fraction": 0.25, "seed": 19},
        "data_version": "dataset-sha256",
    }

    first = train_linear_regression(payload)
    second = train_linear_regression(payload)

    assert first == second
    train_groups = {payload["groups"][index] for index in first["split"]["train_indices"]}
    validation_groups = {payload["groups"][index] for index in first["split"]["validation_indices"]}
    assert train_groups.isdisjoint(validation_groups)
    assert first["data_version"] == "dataset-sha256"
    assert first["metrics"]["validation"]["rmse"] < 0.001


def test_time_split_keeps_future_rows_out_of_training() -> None:
    artifact = train_linear_regression(
        {
            "features": [[0], [1], [2], [3], [4]],
            "targets": [0, 2, 4, 6, 8],
            "split_policy": {"kind": "time", "validation_fraction": 0.4},
        }
    )

    assert artifact["split"]["train_indices"] == [0, 1, 2]
    assert artifact["split"]["validation_indices"] == [3, 4]


def test_missing_values_use_training_statistics_and_model_can_be_reloaded() -> None:
    artifact = train_linear_regression(
        {
            "features": [[0, None], [1, 2], [2, None], [3, 6], [4, 8], [5, 10]],
            "targets": [1, 3, 5, 7, 9, 11],
            "feature_names": ["time", "speed"],
            "split_policy": {"kind": "time", "validation_fraction": 0.33},
        }
    )

    prediction = predict_linear_regression(artifact, [[6, None], [7, 14]])

    assert artifact["preprocessing"]["imputation"] == "train_mean"
    assert len(artifact["feature_importance"]) == 2
    assert prediction["model_id"] == artifact["model_id"]
    assert len(prediction["predictions"]) == 2
    assert all(math.isfinite(value) for value in prediction["predictions"])


def test_training_rejects_unknown_algorithm() -> None:
    with pytest.raises(ValueError, match="unsupported training algorithm"):
        train_linear_regression(
            {
                "features": [[1], [2]],
                "targets": [1, 2],
                "algorithm": "random_forest",
            }
        )
