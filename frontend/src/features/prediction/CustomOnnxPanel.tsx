import { useState } from "react";
import { Box, Loader2, Play } from "lucide-react";
import { desktop, type ComputeOnnxPredictionResult } from "../../bridge/desktop";
import { Input } from "../../components/ui/input";
import { Textarea } from "../../components/ui/textarea";

/**
 * 第 35 章「自定义模型」：用自备的 ONNX 回归模型做预测。
 *
 * 流程对齐 worker 的 `predict_onnx` 契约：选择 .onnx 文件 → 计算并核对
 * sha256 → 提供 manifest（可粘贴导出模型自带的 JSON，或按表单生成）→
 * 提交特征行 → 展示预测值与适用范围告警。
 */

const splitNumbers = (value: string): number[] =>
  value
    .split(/[,\s]+/)
    .filter((item) => item.trim().length > 0)
    .map((item) => Number(item));

const splitNames = (value: string): string[] =>
  value
    .split(/[,\s]+/)
    .map((item) => item.trim())
    .filter((item) => item.length > 0);

export function buildCustomManifest(form: {
  modelName: string;
  inputName: string;
  outputName: string;
  featureNames: string;
  means: string;
  scales: string;
}): { manifest: Record<string, unknown> | null; error: string } {
  const names = splitNames(form.featureNames);
  if (names.length === 0) return { manifest: null, error: "请至少填写一个特征名" };
  const means = form.means.trim() ? splitNumbers(form.means) : names.map(() => 0);
  const scales = form.scales.trim() ? splitNumbers(form.scales) : names.map(() => 1);
  if (means.length !== names.length || scales.length !== names.length) {
    return { manifest: null, error: "均值/标准差的个数必须与特征名一致" };
  }
  if (scales.some((value) => !Number.isFinite(value) || value <= 0)) {
    return { manifest: null, error: "标准差必须是正数" };
  }
  if (means.some((value) => !Number.isFinite(value))) {
    return { manifest: null, error: "均值必须是有效数字" };
  }
  return {
    manifest: {
      model_id: form.modelName.trim() || "custom-onnx",
      model_version: "1.0.0",
      inputs: [{ name: form.inputName.trim() || "X", dtype: "float32", shape: [-1, names.length] }],
      outputs: [{ name: form.outputName.trim() || "Y", dtype: "float32", shape: [-1, 1] }],
      preprocessing: { feature_names: names, means, scales },
    },
    error: "",
  };
}

export default function CustomOnnxPanel({
  onNotice,
  onError,
}: {
  onNotice: (message: string) => void;
  onError: (message: string) => void;
}) {
  const [modelPath, setModelPath] = useState("");
  const [modelSha256, setModelSha256] = useState("");
  const [modelName, setModelName] = useState("");
  const [inputName, setInputName] = useState("X");
  const [outputName, setOutputName] = useState("Y");
  const [featureNames, setFeatureNames] = useState("");
  const [means, setMeans] = useState("");
  const [scales, setScales] = useState("");
  const [manifestText, setManifestText] = useState("");
  const [featureValues, setFeatureValues] = useState("");
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState<ComputeOnnxPredictionResult | null>(null);

  const pickModel = async () => {
    const path = await desktop.openFileDialog({
      multiple: false,
      directory: false,
      filters: [{ name: "ONNX 模型", extensions: ["onnx"] }],
    });
    if (typeof path !== "string") return;
    onError("");
    setModelPath(path);
    setModelSha256("");
    try {
      setModelSha256(await desktop.hashOnnxModelFile(path));
    } catch (reason) {
      onError(reason instanceof Error ? reason.message : "无法读取所选模型文件");
    }
  };

  const run = async () => {
    onError("");
    setResult(null);
    if (!modelPath || !modelSha256) {
      onError("请先选择 ONNX 模型文件（并等待校验和计算完成）");
      return;
    }
    let manifest: Record<string, unknown>;
    if (manifestText.trim()) {
      try {
        const parsed = JSON.parse(manifestText) as unknown;
        if (typeof parsed !== "object" || parsed === null) throw new Error("not an object");
        manifest = parsed as Record<string, unknown>;
      } catch {
        onError("manifest JSON 无法解析");
        return;
      }
    } else {
      const built = buildCustomManifest({ modelName, inputName, outputName, featureNames, means, scales });
      if (built.error || !built.manifest) {
        onError(built.error || "无法生成 manifest");
        return;
      }
      manifest = built.manifest;
    }
    const rows = featureValues
      .split("\n")
      .map((line) => splitNumbers(line))
      .filter((row) => row.length > 0);
    if (rows.length === 0 || rows.some((row) => row.some((value) => !Number.isFinite(value)))) {
      onError("请每行输入一组用逗号分隔的特征值");
      return;
    }
    setRunning(true);
    try {
      const task = await desktop.predictOnnxModel({ modelPath, modelSha256, manifest, features: rows });
      let completed: ComputeOnnxPredictionResult | null = null;
      for (let attempt = 0; attempt < 60 && !completed; attempt += 1) {
        completed = await desktop.getComputeOnnxPredictionResult(task.id);
        if (!completed) await new Promise((resolve) => window.setTimeout(resolve, 600));
      }
      if (!completed) throw new Error("预测任务超时，请在运行记录中查看状态");
      setResult(completed);
      onNotice("自定义 ONNX 模型预测完成");
    } catch (reason) {
      onError(reason instanceof Error ? reason.message : "ONNX 预测失败");
    } finally {
      setRunning(false);
    }
  };

  const predictionRows = result
    ? Array.isArray(result.predictions[0])
      ? (result.predictions as number[][])
      : [result.predictions as number[]]
    : [];

  return (
    <section className="suna-onnx-panel">
      <div className="suna-prediction-section-title">
        <Box size={17} />
        <div>
          <h2>自定义模型（ONNX）</h2>
          <span>加载自备的 ONNX 回归模型；导出模型可直接粘贴其 manifest JSON。</span>
        </div>
      </div>
      <div className="suna-onnx-form">
        <label className="suna-onnx-file">
          模型文件
          <span>
            <button type="button" className="suna-ghost-button" onClick={() => void pickModel()}>
              选择 .onnx 文件
            </button>
            <small>{modelPath ? modelPath.split(/[\\/]/).pop() : "未选择"}</small>
            {modelSha256 && <small>sha256 {modelSha256.slice(0, 12)}…</small>}
          </span>
        </label>
        <label>模型名称<Input value={modelName} onChange={(event) => setModelName(event.target.value)} placeholder="custom-onnx" /></label>
        <label>输入名 / 输出名<span className="suna-onnx-pair">
          <Input value={inputName} onChange={(event) => setInputName(event.target.value)} />
          <Input value={outputName} onChange={(event) => setOutputName(event.target.value)} />
        </span></label>
        <label>特征名（逗号分隔）<Input value={featureNames} onChange={(event) => setFeatureNames(event.target.value)} placeholder="C, Mn, 淬火温度" /></label>
        <label>均值 / 标准差（可选，逗号分隔）<span className="suna-onnx-pair">
          <Input value={means} onChange={(event) => setMeans(event.target.value)} placeholder="默认 0" />
          <Input value={scales} onChange={(event) => setScales(event.target.value)} placeholder="默认 1" />
        </span></label>
        <label>manifest JSON（可选，覆盖上方表单）<Textarea value={manifestText} onChange={(event) => setManifestText(event.target.value)} rows={4} placeholder='{"model_id": "...", "inputs": [...]}' /></label>
        <label>输入特征（每行一组，逗号分隔）<Textarea value={featureValues} onChange={(event) => setFeatureValues(event.target.value)} rows={3} placeholder="0.12, 1.35, 880" /></label>
        <button className="suna-primary-button" onClick={() => void run()} disabled={running}>
          {running ? <Loader2 className="suna-spin" size={15} /> : <Play size={15} />}
          {running ? "预测中..." : "运行预测"}
        </button>
      </div>
      {result && (
        <div className="suna-onnx-result">
          <div className="suna-prediction-metrics">
            <div><span>模型</span><strong>{result.model_id} · v{result.model_version}</strong></div>
            <div><span>opset</span><strong>{result.opset_version}</strong></div>
            <div><span>算子数</span><strong>{result.operators.length}</strong></div>
          </div>
          <div className="suna-optimization-table">
            <div className="suna-optimization-row suna-optimization-row-head"><strong>行</strong><strong>预测值</strong></div>
            {predictionRows.map((row, index) => (
              <div className="suna-optimization-row" key={index}>
                <span>第 {index + 1} 行</span>
                <span>{row.map((value) => Number(value).toFixed(4)).join(", ")}</span>
              </div>
            ))}
          </div>
          {result.applicability_warnings.length > 0 && (
            <p className="suna-onnx-warning">输入超出 manifest 声明的适用范围，结果仅供参考。</p>
          )}
        </div>
      )}
    </section>
  );
}
