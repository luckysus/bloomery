import { describe, expect, it } from "vitest";
import { buildCustomManifest } from "./CustomOnnxPanel";

describe("buildCustomManifest", () => {
  it("builds an identity-preprocessing manifest from the form fields", () => {
    const built = buildCustomManifest({
      modelName: "my-regressor",
      inputName: "X",
      outputName: "Y",
      featureNames: "C, Mn, 淬火温度",
      means: "",
      scales: "",
    });
    expect(built.error).toBe("");
    expect(built.manifest).toEqual({
      model_id: "my-regressor",
      model_version: "1.0.0",
      inputs: [{ name: "X", dtype: "float32", shape: [-1, 3] }],
      outputs: [{ name: "Y", dtype: "float32", shape: [-1, 1] }],
      preprocessing: {
        feature_names: ["C", "Mn", "淬火温度"],
        means: [0, 0, 0],
        scales: [1, 1, 1],
      },
    });
  });

  it("accepts explicit means and scales when they match the feature count", () => {
    const built = buildCustomManifest({
      modelName: "",
      inputName: "",
      outputName: "",
      featureNames: "a,b",
      means: "0.5, 1.5",
      scales: "2,4",
    });
    expect(built.error).toBe("");
    expect(built.manifest?.model_id).toBe("custom-onnx");
    expect(built.manifest?.inputs).toEqual([{ name: "X", dtype: "float32", shape: [-1, 2] }]);
    expect(built.manifest?.preprocessing).toEqual({
      feature_names: ["a", "b"],
      means: [0.5, 1.5],
      scales: [2, 4],
    });
  });

  it("rejects mismatched or invalid statistics", () => {
    const mismatched = buildCustomManifest({
      modelName: "",
      inputName: "X",
      outputName: "Y",
      featureNames: "a,b",
      means: "1",
      scales: "",
    });
    expect(mismatched.error).toContain("个数必须与特征名一致");
    const nonPositive = buildCustomManifest({
      modelName: "",
      inputName: "X",
      outputName: "Y",
      featureNames: "a,b",
      means: "",
      scales: "1, 0",
    });
    expect(nonPositive.error).toContain("标准差必须是正数");
    const empty = buildCustomManifest({
      modelName: "",
      inputName: "X",
      outputName: "Y",
      featureNames: "  ",
      means: "",
      scales: "",
    });
    expect(empty.manifest).toBeNull();
  });
});
