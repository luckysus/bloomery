"""FT-Transformer 风格的表格回归器（第 35 章：模型中心的 Transformer 族）。

纯 numpy 实现，不引入 PyTorch 等重依赖：

- 每个特征一个可学习的线性 token（feature tokenizer），外加一个 CLS token；
- ``LAYERS`` 层 pre-LN Transformer 编码器块（多头自注意力 + GELU 前馈）；
- 最终 LayerNorm 后取 CLS 表达，接线性头输出标量。

确定性：固定种子初始化、全批 Adam、无 dropout —— 同 seed 同数据得到
逐位一致的 artifact。权重以 base64(float64) 存进 artifact 并在预测时
重建，不经过 pickle，也不依赖环境锁。
"""

from __future__ import annotations

import base64
import binascii
import math
import random
from typing import Any, Mapping

import numpy as np

ARTIFACT_VERSION = "transformer.v1"
MODEL_TYPE = "transformer"

D_MODEL = 16
HEADS = 2
LAYERS = 2
D_FF = 32
LEARNING_RATE = 0.01
LN_EPS = 1e-5

DEFAULT_EPOCHS = 120
MIN_EPOCHS = 10
MAX_EPOCHS = 300

# 全批训练的内存/耗时上限：超出后用种子子采样，只影响优化器看到的行，
# 指标仍按完整 train/validation 划分计算并记录在 artifact 中。
MAX_TRAIN_ROWS = 5_000
PREDICT_CHUNK_ROWS = 2048

_GELU_C = math.sqrt(2.0 / math.pi)


def train_transformer(payload: Mapping[str, Any]) -> dict[str, Any]:
    """Fit the tabular transformer and persist weights as base64 arrays."""
    # 延迟导入避免与 training.py 循环依赖（training 在函数内路由到本模块）。
    from .training import (
        _applicability_range,
        _artifact_id,
        _fit_preprocessing,
        _mapping,
        _metrics,
        _split_indices,
        _validate_dataset,
    )

    features, targets, feature_names = _validate_dataset(payload)
    feature_count = len(feature_names)
    if feature_count < 2:
        raise ValueError("transformer requires at least two feature columns")
    if D_MODEL % HEADS != 0:
        raise ValueError("transformer head configuration is invalid")

    seed_value = payload.get("seed", 0)
    if isinstance(seed_value, bool) or not isinstance(seed_value, int):
        raise ValueError("seed must be an integer")
    epochs = payload.get("epochs", DEFAULT_EPOCHS)
    if isinstance(epochs, bool) or not isinstance(epochs, int):
        raise ValueError("epochs must be an integer")
    if not MIN_EPOCHS <= epochs <= MAX_EPOCHS:
        raise ValueError(f"epochs must be between {MIN_EPOCHS} and {MAX_EPOCHS}")

    split = _split_indices(len(features), payload)
    means, scales, transformed = _fit_preprocessing(features, split["train_indices"])

    train_indices = sorted(split["train_indices"])
    subsampled = False
    if len(train_indices) > MAX_TRAIN_ROWS:
        train_indices = sorted(random.Random(seed_value).sample(train_indices, MAX_TRAIN_ROWS))
        subsampled = True

    x_train = np.asarray([transformed[index] for index in train_indices], dtype=np.float64)
    full_train_targets = np.asarray(
        [targets[index] for index in split["train_indices"]], dtype=np.float64
    )
    target_mean = float(full_train_targets.mean())
    target_std = float(full_train_targets.std())
    target_scale = target_std if target_std > 1e-12 else 1.0
    t_train = (
        np.asarray([targets[index] for index in train_indices], dtype=np.float64) - target_mean
    ) / target_scale

    weights = _init_weights(feature_count, seed_value)
    adam_state: dict[str, tuple[np.ndarray, np.ndarray]] = {}
    for epoch in range(epochs):
        _adam_step(x_train, t_train, weights, state=adam_state, step=epoch + 1)

    all_predictions = _predict_raw(weights, np.asarray(transformed, dtype=np.float64))
    predictions = [float(value) * target_scale + target_mean for value in all_predictions]
    if not all(math.isfinite(value) for value in predictions):
        raise ValueError("transformer training produced non-finite predictions")

    artifact: dict[str, Any] = {
        "artifact_version": ARTIFACT_VERSION,
        "model_type": MODEL_TYPE,
        "data_version": str(payload.get("data_version", "unknown")),
        "feature_names": feature_names,
        "feature_schema": {"count": feature_count, "names": feature_names},
        "field_mapping": _mapping(payload.get("field_mapping", {})),
        "preprocessing": {
            "fit_scope": "train_only",
            "imputation": "train_mean",
            "means": means,
            "scales": scales,
            "target_scaling": {"mean": target_mean, "scale": target_scale},
        },
        "parameters": {
            "algorithm": MODEL_TYPE,
            "seed": seed_value,
            "epochs": epochs,
            "learning_rate": LEARNING_RATE,
            "d_model": D_MODEL,
            "heads": HEADS,
            "layers": LAYERS,
            "d_ff": D_FF,
            "train_rows": len(train_indices),
            "train_subsampled": subsampled,
        },
        "split": split,
        "metrics": {
            "train": _metrics(targets, predictions, split["train_indices"]),
            "validation": _metrics(targets, predictions, split["validation_indices"]),
        },
        "applicability_range": _applicability_range(features, split["train_indices"]),
        "weights_base64": _encode_weights(weights),
    }
    artifact["model_id"] = _artifact_id(artifact)
    return artifact


def predict_transformer(artifact: Mapping[str, Any], features: Any) -> dict[str, Any]:
    """Run the trained transformer over raw feature rows."""
    if artifact.get("artifact_version") != ARTIFACT_VERSION:
        raise ValueError("unsupported model artifact version")
    if artifact.get("model_type") != MODEL_TYPE:
        raise ValueError("unsupported model type")
    names = artifact.get("feature_names")
    if not isinstance(names, list) or not names:
        raise ValueError("model artifact schema is invalid")
    feature_count = len(names)
    preprocessing = artifact.get("preprocessing")
    if not isinstance(preprocessing, Mapping):
        raise ValueError("model artifact schema is invalid")
    means = preprocessing.get("means")
    scales = preprocessing.get("scales")
    if (
        not isinstance(means, list)
        or not isinstance(scales, list)
        or len(means) != feature_count
        or len(scales) != feature_count
    ):
        raise ValueError("model preprocessing schema is invalid")
    target_scaling = preprocessing.get("target_scaling")
    if not isinstance(target_scaling, Mapping):
        raise ValueError("model preprocessing schema is invalid")
    target_mean = float(target_scaling.get("mean", 0.0))
    target_scale = float(target_scaling.get("scale", 1.0))
    if not math.isfinite(target_mean) or not math.isfinite(target_scale) or target_scale == 0:
        raise ValueError("model preprocessing schema is invalid")

    from .training import _normalise_features

    raw_rows = _normalise_features(features, feature_count)
    rows = [
        [
            ((value if value is not None else float(means[column])) - float(means[column]))
            / float(scales[column])
            for column, value in enumerate(row)
        ]
        for row in raw_rows
    ]
    weights = _decode_weights(artifact.get("weights_base64"), feature_count)
    predictions: list[float] = []
    for start in range(0, len(rows), PREDICT_CHUNK_ROWS):
        chunk = np.asarray(rows[start : start + PREDICT_CHUNK_ROWS], dtype=np.float64)
        for value in _predict_raw(weights, chunk):
            number = float(value) * target_scale + target_mean
            if not math.isfinite(number):
                raise ValueError("model predictions must be finite")
            predictions.append(number)
    return {
        "model_id": str(artifact.get("model_id", "")),
        "model_type": MODEL_TYPE,
        "predictions": predictions,
        "feature_names": names,
    }


# ---------------------------------------------------------------------------
# 权重存取


def _encode_weights(weights: dict[str, np.ndarray]) -> dict[str, dict[str, Any]]:
    return {
        name: {
            "shape": list(value.shape),
            "data": base64.b64encode(
                np.ascontiguousarray(value, dtype="<f8").tobytes()
            ).decode("ascii"),
        }
        for name, value in sorted(weights.items())
    }


def _decode_weights(stored: Any, feature_count: int) -> dict[str, np.ndarray]:
    expected = _weight_shapes(feature_count)
    if not isinstance(stored, Mapping) or set(stored) != set(expected):
        raise ValueError("model weights schema is invalid")
    weights: dict[str, np.ndarray] = {}
    for name, shape in expected.items():
        entry = stored[name]
        if not isinstance(entry, Mapping) or list(entry.get("shape", [])) != list(shape):
            raise ValueError(f"model weight {name} has an invalid shape")
        try:
            raw = base64.b64decode(str(entry.get("data", "")), validate=True)
        except (ValueError, binascii.Error) as error:
            raise ValueError(f"model weight {name} encoding is invalid") from error
        if len(raw) != 8 * math.prod(shape):
            raise ValueError(f"model weight {name} has an invalid size")
        weights[name] = np.frombuffer(raw, dtype="<f8").reshape(shape).astype(np.float64)
    return weights


def _weight_shapes(feature_count: int) -> dict[str, tuple[int, ...]]:
    shapes: dict[str, tuple[int, ...]] = {
        "W_emb": (feature_count, D_MODEL),
        "b_emb": (D_MODEL,),
        "cls": (D_MODEL,),
        "lnf.g": (D_MODEL,),
        "lnf.b": (D_MODEL,),
        "W_head": (D_MODEL,),
        "b_head": (1,),
    }
    for layer in range(LAYERS):
        prefix = f"L{layer}."
        shapes.update(
            {
                f"{prefix}ln1.g": (D_MODEL,),
                f"{prefix}ln1.b": (D_MODEL,),
                f"{prefix}Wq": (D_MODEL, D_MODEL),
                f"{prefix}Wk": (D_MODEL, D_MODEL),
                f"{prefix}Wv": (D_MODEL, D_MODEL),
                f"{prefix}Wo": (D_MODEL, D_MODEL),
                f"{prefix}ln2.g": (D_MODEL,),
                f"{prefix}ln2.b": (D_MODEL,),
                f"{prefix}W1": (D_MODEL, D_FF),
                f"{prefix}c1": (D_FF,),
                f"{prefix}W2": (D_FF, D_MODEL),
                f"{prefix}c2": (D_MODEL,),
            }
        )
    return shapes


def _init_weights(feature_count: int, seed: int) -> dict[str, np.ndarray]:
    rng = np.random.default_rng(seed)

    def matrix(rows: int, columns: int) -> np.ndarray:
        return rng.normal(0.0, 1.0 / math.sqrt(columns), size=(rows, columns))

    def vector(size: int, scale: float = 0.0) -> np.ndarray:
        return rng.normal(0.0, scale, size=size) if scale > 0 else np.zeros(size)

    weights: dict[str, np.ndarray] = {
        "W_emb": matrix(feature_count, D_MODEL),
        "b_emb": vector(D_MODEL),
        "cls": vector(D_MODEL),
        "lnf.g": np.ones(D_MODEL),
        "lnf.b": vector(D_MODEL),
        "W_head": vector(D_MODEL, 1.0 / math.sqrt(D_MODEL)),
        "b_head": np.zeros(1),
    }
    for layer in range(LAYERS):
        prefix = f"L{layer}."
        weights.update(
            {
                f"{prefix}ln1.g": np.ones(D_MODEL),
                f"{prefix}ln1.b": vector(D_MODEL),
                f"{prefix}Wq": matrix(D_MODEL, D_MODEL),
                f"{prefix}Wk": matrix(D_MODEL, D_MODEL),
                f"{prefix}Wv": matrix(D_MODEL, D_MODEL),
                f"{prefix}Wo": matrix(D_MODEL, D_MODEL),
                f"{prefix}ln2.g": np.ones(D_MODEL),
                f"{prefix}ln2.b": vector(D_MODEL),
                f"{prefix}W1": matrix(D_MODEL, D_FF),
                f"{prefix}c1": vector(D_FF),
                f"{prefix}W2": matrix(D_FF, D_MODEL),
                f"{prefix}c2": vector(D_MODEL),
            }
        )
    return weights


# ---------------------------------------------------------------------------
# 前向 / 反向
#
# 约定：``caches[layer]`` 是第 layer 层的前向缓存（0 起），
# ``caches[LAYERS]`` 是最终 LayerNorm 与 CLS 的缓存。


def _gelu(values: np.ndarray) -> np.ndarray:
    return 0.5 * values * (1.0 + np.tanh(_GELU_C * (values + 0.044715 * values**3)))


def _gelu_grad(values: np.ndarray) -> np.ndarray:
    inner = _GELU_C * (values + 0.044715 * values**3)
    tanh = np.tanh(inner)
    return 0.5 * (1.0 + tanh) + 0.5 * values * (1.0 - tanh * tanh) * _GELU_C * (
        1.0 + 3 * 0.044715 * values**2
    )


def _layer_norm(
    values: np.ndarray, gain: np.ndarray, bias: np.ndarray
) -> tuple[np.ndarray, dict[str, np.ndarray]]:
    mean = values.mean(axis=-1, keepdims=True)
    centred = values - mean
    variance = (centred**2).mean(axis=-1, keepdims=True)
    inverse = 1.0 / np.sqrt(variance + LN_EPS)
    normalised = centred * inverse
    return gain * normalised + bias, {"normalised": normalised, "inverse": inverse}


def _layer_norm_grad(
    dout: np.ndarray, cache: dict[str, np.ndarray], gain: np.ndarray
) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    normalised = cache["normalised"]
    inverse = cache["inverse"]
    d_gain = (dout * normalised).sum(axis=(0, 1))
    d_bias = dout.sum(axis=(0, 1))
    d_xhat = dout * gain
    d_values = inverse * (
        d_xhat
        - d_xhat.mean(axis=-1, keepdims=True)
        - normalised * (d_xhat * normalised).mean(axis=-1, keepdims=True)
    )
    return d_values, d_gain, d_bias


def _softmax(scores: np.ndarray) -> np.ndarray:
    shifted = scores - scores.max(axis=-1, keepdims=True)
    exponent = np.exp(shifted)
    return exponent / exponent.sum(axis=-1, keepdims=True)


def _forward(
    x: np.ndarray, weights: dict[str, np.ndarray]
) -> tuple[np.ndarray, list[dict[str, Any]]]:
    """Run the encoder over standardised rows and return per-row CLS logits."""
    rows, feature_count = x.shape
    tokens = feature_count + 1
    head_dim = D_MODEL // HEADS
    # feature tokenizer：emb[n, f, :] = x[n, f] * W_emb[f, :] + b_emb
    embedded = np.einsum("nf,fd->nfd", x, weights["W_emb"]) + weights["b_emb"]
    hidden = np.concatenate(
        [np.broadcast_to(weights["cls"], (rows, 1, D_MODEL)), embedded], axis=1
    )

    caches: list[dict[str, Any]] = []
    for layer in range(LAYERS):
        prefix = f"L{layer}."
        hidden_in = hidden
        normalised, ln1_cache = _layer_norm(
            hidden_in, weights[f"{prefix}ln1.g"], weights[f"{prefix}ln1.b"]
        )
        queries = normalised @ weights[f"{prefix}Wq"]
        keys = normalised @ weights[f"{prefix}Wk"]
        values = normalised @ weights[f"{prefix}Wv"]
        q_heads = queries.reshape(rows, tokens, HEADS, head_dim).transpose(0, 2, 1, 3)
        k_heads = keys.reshape(rows, tokens, HEADS, head_dim).transpose(0, 2, 1, 3)
        v_heads = values.reshape(rows, tokens, HEADS, head_dim).transpose(0, 2, 1, 3)
        scores = np.einsum("nhtd,nhsd->nhts", q_heads, k_heads) / math.sqrt(head_dim)
        attention = _softmax(scores)
        attended = np.einsum("nhts,nhsd->nhtd", attention, v_heads)
        attended_flat = attended.transpose(0, 2, 1, 3).reshape(rows, tokens, D_MODEL)
        attention_out = attended_flat @ weights[f"{prefix}Wo"]
        hidden_mid = hidden_in + attention_out

        normalised2, ln2_cache = _layer_norm(
            hidden_mid, weights[f"{prefix}ln2.g"], weights[f"{prefix}ln2.b"]
        )
        z1 = normalised2 @ weights[f"{prefix}W1"] + weights[f"{prefix}c1"]
        activated = _gelu(z1)
        ffn_out = activated @ weights[f"{prefix}W2"] + weights[f"{prefix}c2"]
        hidden = hidden_mid + ffn_out

        caches.append(
            {
                "ln1": ln1_cache,
                "normalised": normalised,
                "q_heads": q_heads,
                "k_heads": k_heads,
                "v_heads": v_heads,
                "attention": attention,
                "attended_flat": attended_flat,
                "ln2": ln2_cache,
                "normalised2": normalised2,
                "z1": z1,
                "activated": activated,
            }
        )

    final_normalised, lnf_cache = _layer_norm(hidden, weights["lnf.g"], weights["lnf.b"])
    cls = final_normalised[:, 0, :]
    logits = cls @ weights["W_head"] + weights["b_head"][0]
    caches.append({"lnf": lnf_cache, "cls": cls})
    return logits, caches


def _backward(
    x: np.ndarray, targets: np.ndarray, weights: dict[str, np.ndarray]
) -> tuple[float, dict[str, np.ndarray]]:
    """MSE 损失与解析梯度；梯度检查测试会用有限差分校验每一个参数。"""
    rows, feature_count = x.shape
    tokens = feature_count + 1
    head_dim = D_MODEL // HEADS
    logits, caches = _forward(x, weights)
    loss = float(((logits - targets) ** 2).mean())

    grad: dict[str, np.ndarray] = {}
    d_logits = 2.0 * (logits - targets) / rows
    final_cache = caches[LAYERS]
    grad["W_head"] = final_cache["cls"].T @ d_logits
    grad["b_head"] = np.asarray([d_logits.sum()])
    d_cls = d_logits[:, None] * weights["W_head"][None, :]
    d_hidden = np.zeros((rows, tokens, D_MODEL))
    d_hidden[:, 0, :] = d_cls
    d_hidden, d_lnf_g, d_lnf_b = _layer_norm_grad(
        d_hidden, final_cache["lnf"], weights["lnf.g"]
    )
    grad["lnf.g"] = d_lnf_g
    grad["lnf.b"] = d_lnf_b

    for layer in range(LAYERS - 1, -1, -1):
        prefix = f"L{layer}."
        cache = caches[layer]
        # 进入本层输出 hidden_out 的梯度（来自上层与最终 LN）。
        d_h_out = d_hidden

        # 前馈分支：hidden_out = hidden_mid + FFN(LN2(hidden_mid))
        grad[f"{prefix}W2"] = (
            cache["activated"].reshape(-1, D_FF).T @ d_h_out.reshape(-1, D_MODEL)
        )
        grad[f"{prefix}c2"] = d_h_out.sum(axis=(0, 1))
        d_activated = d_h_out @ weights[f"{prefix}W2"].T
        d_z1 = d_activated * _gelu_grad(cache["z1"])
        grad[f"{prefix}W1"] = (
            cache["normalised2"].reshape(-1, D_MODEL).T @ d_z1.reshape(-1, D_FF)
        )
        grad[f"{prefix}c1"] = d_z1.sum(axis=(0, 1))
        d_normalised2 = d_z1 @ weights[f"{prefix}W1"].T
        d_hm_from_ln2, d_ln2_g, d_ln2_b = _layer_norm_grad(
            d_normalised2, cache["ln2"], weights[f"{prefix}ln2.g"]
        )
        grad[f"{prefix}ln2.g"] = d_ln2_g
        grad[f"{prefix}ln2.b"] = d_ln2_b
        d_hidden_mid = d_h_out + d_hm_from_ln2

        # 注意力分支：hidden_mid = hidden_in + MHA(LN1(hidden_in))
        grad[f"{prefix}Wo"] = (
            cache["attended_flat"].reshape(-1, D_MODEL).T @ d_hidden_mid.reshape(-1, D_MODEL)
        )
        d_attended_flat = d_hidden_mid @ weights[f"{prefix}Wo"].T
        d_attended = d_attended_flat.reshape(rows, tokens, HEADS, head_dim).transpose(0, 2, 1, 3)
        attention = cache["attention"]
        # O = A @ V：输出梯度先分配到注意力权重 A 上，再做 softmax 反传。
        d_attention_weights = np.einsum("nhtd,nhsd->nhts", d_attended, cache["v_heads"])
        d_scores = attention * (
            d_attention_weights - (d_attention_weights * attention).sum(axis=-1, keepdims=True)
        )
        d_q_heads = np.einsum("nhts,nhsd->nhtd", d_scores, cache["k_heads"]) / math.sqrt(head_dim)
        d_k_heads = np.einsum("nhts,nhtd->nhsd", d_scores, cache["q_heads"]) / math.sqrt(head_dim)
        d_v_heads = np.einsum("nhts,nhtd->nhsd", attention, d_attended)
        d_queries = d_q_heads.transpose(0, 2, 1, 3).reshape(rows, tokens, D_MODEL)
        d_keys = d_k_heads.transpose(0, 2, 1, 3).reshape(rows, tokens, D_MODEL)
        d_values = d_v_heads.transpose(0, 2, 1, 3).reshape(rows, tokens, D_MODEL)
        normalised_flat = cache["normalised"].reshape(-1, D_MODEL)
        grad[f"{prefix}Wq"] = normalised_flat.T @ d_queries.reshape(-1, D_MODEL)
        grad[f"{prefix}Wk"] = normalised_flat.T @ d_keys.reshape(-1, D_MODEL)
        grad[f"{prefix}Wv"] = normalised_flat.T @ d_values.reshape(-1, D_MODEL)
        d_normalised = d_queries @ weights[f"{prefix}Wq"].T
        d_normalised += d_keys @ weights[f"{prefix}Wk"].T
        d_normalised += d_values @ weights[f"{prefix}Wv"].T
        d_hidden_from_ln1, d_ln1_g, d_ln1_b = _layer_norm_grad(
            d_normalised, cache["ln1"], weights[f"{prefix}ln1.g"]
        )
        grad[f"{prefix}ln1.g"] = d_ln1_g
        grad[f"{prefix}ln1.b"] = d_ln1_b
        # hidden_in 同时流向 LN1（经注意力）与残差相加。
        d_hidden = d_hidden_mid + d_hidden_from_ln1

    # feature tokenizer：emb[n, f, :] = x[n, f] * W_emb[f, :] + b_emb
    grad["b_emb"] = d_hidden[:, 1:, :].sum(axis=(0, 1))
    grad["W_emb"] = np.einsum("nf,nfd->fd", x, d_hidden[:, 1:, :])
    grad["cls"] = d_hidden[:, 0, :].sum(axis=0)
    assert d_hidden.shape == (rows, tokens, D_MODEL)
    return loss, grad


def _adam_step(
    x: np.ndarray,
    targets: np.ndarray,
    weights: dict[str, np.ndarray],
    state: dict[str, tuple[np.ndarray, np.ndarray]] | None = None,
    step: int = 1,
) -> None:
    """Full-batch Adam update。``state``/``step`` 供梯度检查测试复用动量缓存。"""
    _, grad = _backward(x, targets, weights)
    beta1, beta2, epsilon = 0.9, 0.999, 1e-8
    if state is None:
        state = {}
    for name, value in weights.items():
        moments = state.get(name)
        if moments is None:
            moments = (np.zeros_like(value), np.zeros_like(value))
            state[name] = moments
        first, second = moments
        first *= beta1
        first += (1.0 - beta1) * grad[name]
        second *= beta2
        second += (1.0 - beta2) * grad[name] ** 2
        first_hat = first / (1.0 - beta1**step)
        second_hat = second / (1.0 - beta2**step)
        value -= LEARNING_RATE * first_hat / (np.sqrt(second_hat) + epsilon)


def _predict_raw(weights: dict[str, np.ndarray], x: np.ndarray) -> np.ndarray:
    logits, _ = _forward(x, weights)
    return logits
