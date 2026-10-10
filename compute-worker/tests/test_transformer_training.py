import base64

import numpy as np
import pytest

from suna_worker.training import predict_model, train_sklearn_model
from suna_worker.transformer import (
    ARTIFACT_VERSION,
    D_FF,
    D_MODEL,
    HEADS,
    LAYERS,
    MAX_EPOCHS,
    MAX_TRAIN_ROWS,
    MIN_EPOCHS,
    _backward,
    _init_weights,
    _weight_shapes,
    predict_transformer,
    train_transformer,
)


def dataset(rows=80):
    features = [[float(i), float(i % 7), float((i * 3) % 11)] for i in range(rows)]
    targets = [2.0 * a - 1.5 * b + 0.5 * c + 3.0 for a, b, c in features]
    return {
        "features": features,
        "targets": targets,
        "feature_names": ["temperature", "carbon", "manganese"],
        "split_policy": {"kind": "random", "validation_fraction": 0.25, "seed": 11},
        "algorithm": "transformer",
        "seed": 11,
        "epochs": 120,
    }


def test_gradients_match_finite_differences_on_a_tiny_model():
    rng = np.random.default_rng(3)
    rows, feature_count = 6, 3
    x = rng.normal(size=(rows, feature_count))
    targets = rng.normal(size=rows)
    weights = _init_weights(feature_count, 5)
    # 中心差分误差 ~ h²f'''/6；cls 初始为零、曲率高，用相对容差判定。
    _, analytic = _backward(x, targets, weights)
    epsilon = 1e-5
    for name, value in weights.items():
        flat = value.reshape(-1)
        analytic_flat = analytic[name].reshape(-1)
        for index in range(flat.size):
            original = flat[index]
            flat[index] = original + epsilon
            loss_plus, _ = _backward(x, targets, weights)
            flat[index] = original - epsilon
            loss_minus, _ = _backward(x, targets, weights)
            flat[index] = original
            numeric = (loss_plus - loss_minus) / (2 * epsilon)
            deviation = abs(numeric - analytic_flat[index]) / max(1e-3, abs(numeric))
            assert deviation < 1e-4, (
                f"gradient mismatch for {name}[{index}]: analytic={analytic_flat[index]} numeric={numeric}"
            )


def test_training_is_seed_deterministic():
    first = train_transformer(dataset())
    second = train_transformer(dataset())
    assert first["model_id"] == second["model_id"]
    assert first["weights_base64"] == second["weights_base64"]


def test_artifact_shape_and_weight_schema():
    artifact = train_transformer(dataset())
    assert artifact["artifact_version"] == ARTIFACT_VERSION
    assert artifact["model_type"] == "transformer"
    assert artifact["parameters"]["algorithm"] == "transformer"
    assert artifact["parameters"]["train_rows"] == 60  # 80 行、25% 验证集
    assert artifact["parameters"]["train_subsampled"] is False
    expected = _weight_shapes(3)
    assert set(artifact["weights_base64"]) == set(expected)
    for name, entry in artifact["weights_base64"].items():
        assert entry["shape"] == list(expected[name])
        decoded = base64.b64decode(entry["data"], validate=True)
        assert len(decoded) == 8 * np.prod(expected[name])
    # 指标基于完整 train/validation 划分计算。
    assert artifact["metrics"]["train"]["sample_count"] == 60
    assert artifact["metrics"]["validation"]["sample_count"] == 20
    assert artifact["metrics"]["validation"]["r2"] is not None


def test_transformer_tracks_a_linear_target():
    artifact = train_transformer(dataset())
    prediction = predict_model(artifact, [[10.0, 2.0, 4.0]])
    expected = 2.0 * 10.0 - 1.5 * 2.0 + 0.5 * 4.0 + 3.0
    assert abs(prediction["predictions"][0] - expected) < 3.0
    assert prediction["model_type"] == "transformer"
    assert prediction["feature_names"] == ["temperature", "carbon", "manganese"]


def test_predict_transformer_round_trip_via_training_dispatch():
    payload = dataset()
    payload["algorithm"] = "transformer"
    artifact = train_sklearn_model(payload)
    assert artifact["artifact_version"] == ARTIFACT_VERSION
    direct = predict_transformer(artifact, [[12.0, 3.0, 6.0], [4.0, 1.0, 2.0]])
    dispatched = predict_model(artifact, [[12.0, 3.0, 6.0], [4.0, 1.0, 2.0]])
    assert dispatched["predictions"] == direct["predictions"]
    assert abs(dispatched["predictions"][0] - (24.0 - 4.5 + 3.0 + 3.0)) < 3.0


def test_large_training_sets_are_subsampled_deterministically():
    payload = dataset(rows=6000)
    payload["split_policy"] = {"kind": "random", "validation_fraction": 0.05, "seed": 11}
    payload["epochs"] = MIN_EPOCHS
    artifact = train_transformer(payload)
    assert artifact["parameters"]["train_subsampled"] is True
    assert artifact["parameters"]["train_rows"] == MAX_TRAIN_ROWS
    assert artifact["metrics"]["train"]["sample_count"] == 5700


def test_invalid_hyperparameters_are_rejected():
    payload = dataset()
    payload["epochs"] = MIN_EPOCHS - 1
    with pytest.raises(ValueError, match="epochs"):
        train_transformer(payload)
    payload["epochs"] = MAX_EPOCHS + 1
    with pytest.raises(ValueError, match="epochs"):
        train_transformer(payload)
    payload["epochs"] = "many"
    with pytest.raises(ValueError, match="epochs"):
        train_transformer(payload)
    payload["epochs"] = 120
    payload["seed"] = 1.5
    with pytest.raises(ValueError, match="seed"):
        train_transformer(payload)


def test_single_feature_is_rejected():
    payload = dataset()
    payload["features"] = [[float(i)] for i in range(40)]
    payload["targets"] = [float(i) for i in range(40)]
    payload["feature_names"] = ["only"]
    with pytest.raises(ValueError, match="two feature columns"):
        train_transformer(payload)


def test_missing_values_are_imputed_with_train_means():
    payload = dataset()
    payload["features"][0] = [None, 1.0, 2.0]
    artifact = train_transformer(payload)
    prediction = predict_transformer(artifact, [[None, 2.0, 3.0]])
    assert len(prediction["predictions"]) == 1


def test_prediction_rejects_bad_artifacts_and_inputs():
    artifact = train_transformer(dataset())
    with pytest.raises(ValueError, match="exactly 3 columns"):
        predict_transformer(artifact, [[1.0, 2.0]])
    with pytest.raises(ValueError, match="artifact version"):
        predict_transformer({**artifact, "artifact_version": "other.v1"}, [[1.0, 2.0, 3.0]])
    corrupted = {**artifact, "weights_base64": {"W_emb": {"shape": [1, 1], "data": "AAAA"}}}
    with pytest.raises(ValueError, match="weights schema"):
        predict_transformer(corrupted, [[1.0, 2.0, 3.0]])
    with pytest.raises(ValueError, match="exactly 3 columns"):
        predict_transformer(artifact, [[1.0, 2.0]])


def test_architecture_constants_are_self_consistent():
    assert D_MODEL % HEADS == 0
    shapes = _weight_shapes(5)
    assert shapes["W_emb"] == (5, D_MODEL)
    assert shapes["L0.W1"] == (D_MODEL, D_FF)
    assert shapes["L1.W2"] == (D_FF, D_MODEL)
    assert shapes["b_head"] == (1,)
