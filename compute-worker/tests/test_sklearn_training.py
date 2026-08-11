import base64
import os
import pickle

import pytest

from bloomery_worker.training import (
    MAX_PICKLE_BYTES,
    environment_lock,
    predict_model,
    train_sklearn_model,
    xgboost_available,
)


def dataset(rows=60):
    features = [[float(i), float(i % 7)] for i in range(rows)]
    targets = [3.0 * a + 2.0 * b + 1.0 for a, b in features]
    return {
        "features": features,
        "targets": targets,
        "feature_names": ["temperature", "carbon"],
        "split_policy": {"kind": "random", "validation_fraction": 0.25, "seed": 11},
        "seed": 11,
    }


@pytest.mark.parametrize("algorithm", ["elasticnet", "random_forest", "hist_gradient_boosting"])
def test_sklearn_pipelines_train_and_predict_round_trip(algorithm):
    payload = dataset()
    payload["algorithm"] = algorithm
    artifact = train_sklearn_model(payload)

    assert artifact["artifact_version"] == "sklearn-pickle.v1"
    assert artifact["model_type"] == algorithm
    assert artifact["environment"]["lock_sha256"] == environment_lock()["lock_sha256"]
    assert len(artifact["feature_importance"]) == 2
    assert artifact["metrics"]["train"]["rmse"] is not None

    prediction = predict_model(artifact, [[10.0, 2.0], [20.0, 3.0]])
    expected = predict_model(artifact, [[10.0, 2.0], [20.0, 3.0]])
    assert prediction["predictions"] == expected["predictions"]
    assert prediction["model_type"] == algorithm
    # The synthetic target is nearly linear; every pipeline must track it.
    assert abs(prediction["predictions"][0] - (3.0 * 10.0 + 2.0 * 2.0 + 1.0)) < 8.0


def test_random_forest_is_seed_deterministic():
    payload = dataset()
    payload["algorithm"] = "random_forest"
    payload["n_estimators"] = 25
    first = train_sklearn_model(payload)
    second = train_sklearn_model(payload)
    assert first["model_id"] == second["model_id"]
    assert predict_model(first, [[5.0, 1.0]])["predictions"] == predict_model(
        second, [[5.0, 1.0]]
    )["predictions"]


def test_linear_algorithm_still_produces_linear_artifact():
    payload = dataset()
    payload["algorithm"] = "linear_regression"
    artifact = train_sklearn_model(payload)
    assert artifact["artifact_version"] == "linear-regression.v1"
    assert "model_pickle_base64" not in artifact


def test_unknown_algorithm_is_rejected():
    payload = dataset()
    payload["algorithm"] = "deep_magic"
    with pytest.raises(ValueError, match="unsupported training algorithm"):
        train_sklearn_model(payload)


@pytest.mark.parametrize("algorithm", [[], {}])
def test_non_string_algorithm_is_rejected_as_a_value_error(algorithm):
    payload = dataset()
    payload["algorithm"] = algorithm
    with pytest.raises(ValueError, match="unsupported training algorithm"):
        train_sklearn_model(payload)


def test_elasticnet_hyperparameters_are_validated():
    payload = dataset()
    payload["algorithm"] = "elasticnet"
    payload["alpha"] = -1.0
    with pytest.raises(ValueError, match="elasticnet requires"):
        train_sklearn_model(payload)


def test_predict_model_rejects_unknown_artifact_versions():
    with pytest.raises(ValueError, match="unsupported model artifact version"):
        predict_model({"artifact_version": "bogus"}, [[1.0]])


def test_predict_model_rejects_an_oversized_pickle_blob():
    artifact = {
        "artifact_version": "sklearn-pickle.v1",
        "model_type": "random_forest",
        "feature_names": ["temperature"],
        "preprocessing": {"means": [0.0], "scales": [1.0]},
        "environment": environment_lock(),
        "model_pickle_base64": base64.b64encode(
            b"x" * (MAX_PICKLE_BYTES + 1)
        ).decode("ascii"),
    }

    with pytest.raises(ValueError, match="model artifact is too large"):
        predict_model(artifact, [[1.0]])


def test_predict_model_rejects_a_different_runtime_environment():
    payload = dataset()
    payload["algorithm"] = "random_forest"
    artifact = train_sklearn_model(payload)
    artifact["environment"] = {
        **artifact["environment"],
        "python": "0.0.0",
    }

    with pytest.raises(ValueError, match="environment lock"):
        predict_model(artifact, [[1.0, 2.0]])


def test_predict_model_rejects_an_unapproved_model_type():
    artifact = {
        "artifact_version": "sklearn-pickle.v1",
        "model_type": "xgboost",
        "feature_names": ["temperature"],
        "preprocessing": {"means": [0.0], "scales": [1.0]},
        "environment": environment_lock(),
        "model_pickle_base64": base64.b64encode(b"fixture").decode("ascii"),
    }

    with pytest.raises(ValueError, match="unsupported model type"):
        predict_model(artifact, [[1.0]])


@pytest.mark.parametrize(
    ("values", "message"),
    [([float("nan")], "finite"), ([1.0, 2.0], "row count")],
)
def test_predict_model_rejects_invalid_prediction_outputs(
    monkeypatch, values, message
):
    class FakeModel:
        def predict(self, rows):
            return values

    monkeypatch.setattr(
        "bloomery_worker.training._load_trusted_model_pickle",
        lambda blob: FakeModel(),
    )
    artifact = {
        "artifact_version": "sklearn-pickle.v1",
        "model_type": "random_forest",
        "feature_names": ["temperature"],
        "preprocessing": {"means": [0.0], "scales": [1.0]},
        "environment": environment_lock(),
        "model_pickle_base64": base64.b64encode(b"fixture").decode("ascii"),
    }

    with pytest.raises(ValueError, match=message):
        predict_model(artifact, [[1.0]])


def test_predict_model_rejects_untrusted_pickle_without_executing_globals(tmp_path):
    marker = tmp_path / "pickle-executed.txt"

    class Malicious:
        def __reduce__(self):
            return (os.system, (f'echo owned > "{marker}"',))

    artifact = {
        "artifact_version": "sklearn-pickle.v1",
        "model_type": "random_forest",
        "feature_names": ["temperature"],
        "preprocessing": {"means": [0.0], "scales": [1.0]},
        "environment": environment_lock(),
        "model_pickle_base64": base64.b64encode(pickle.dumps(Malicious())).decode("ascii"),
    }

    with pytest.raises(ValueError, match="model artifact could not be loaded"):
        predict_model(artifact, [[1.0]])
    assert not marker.exists()


def test_xgboost_capability_is_explicit_not_inferred():
    # The environment does not install xgboost; the capability flag must say so.
    try:
        import xgboost  # noqa: F401

        assert xgboost_available() is True
    except ImportError:
        assert xgboost_available() is False
