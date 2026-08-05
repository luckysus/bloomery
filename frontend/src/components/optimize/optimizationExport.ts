import * as XLSX from "xlsx";

interface ExportOptimizationSchemeOptions {
  steelMark: string;
  result: any;
  displayProcess: Record<string, any> | null | undefined;
  displayPerf: Record<string, number> | null | undefined;
  locale?: "zh-CN" | "en-US";
}

function exportLabels(locale: "zh-CN" | "en-US") {
  const english = locale === "en-US";
  return {
    parameter: english ? "Parameter" : "参数",
    value: english ? "Value" : "值",
    optimalScheme: english ? "Optimal scheme" : "最优方案",
    paretoSet: english ? "Pareto set" : "帕累托解集",
    filePrefix: english ? "optimization" : "优化方案",
    performance: {
      yield_strength: english ? "Yield strength" : "屈服强度",
      tensile_strength: english ? "Tensile strength" : "抗拉强度",
      elongation: english ? "Elongation" : "延伸率",
    },
  };
}

export function exportOptimizationScheme({ steelMark, result, displayProcess, displayPerf, locale = "zh-CN" }: ExportOptimizationSchemeOptions) {
  const labels = exportLabels(locale);
  const rows: Record<string, string>[] = [];
  if (result.used_composition) {
    Object.entries(result.used_composition).forEach(([key, value]: [string, any]) => {
      rows.push({ [labels.parameter]: key, [labels.value]: value != null ? String(value) : "—" });
    });
  }
  if (displayProcess) {
    Object.entries(displayProcess).forEach(([key, value]: [string, any]) => {
      rows.push({ [labels.parameter]: key, [labels.value]: typeof value === "number" ? value.toFixed(2) : String(value ?? "—") });
    });
  }
  if (displayPerf) {
    Object.entries(labels.performance).forEach(([key, label]) => {
      const unit = key === "elongation" ? "%" : "MPa";
      const value = displayPerf?.[key];
      rows.push({ [labels.parameter]: label, [labels.value]: value != null ? `${value.toFixed(2)} ${unit}` : "—" });
    });
  }

  const worksheet = XLSX.utils.json_to_sheet(rows);
  worksheet["!cols"] = [{ wch: 16 }, { wch: 20 }];
  const workbook = XLSX.utils.book_new();
  XLSX.utils.book_append_sheet(workbook, worksheet, labels.optimalScheme);

  if (result.pareto_front && result.pareto_front.length > 0) {
    const paretoArr = result.pareto_front as Array<{
      predicted_performance: Record<string, number>;
      optimal_process: Record<string, number>;
    }>;
    const compKeys = result.used_composition ? Object.keys(result.used_composition) : [];
    const processKeys = paretoArr[0]?.optimal_process ? Object.keys(paretoArr[0].optimal_process) : [];
    const perfKeys = [
      ["yield_strength", `${labels.performance.yield_strength}(MPa)`],
      ["tensile_strength", `${labels.performance.tensile_strength}(MPa)`],
      ["elongation", `${labels.performance.elongation}(%)`],
    ];
    const headers = [...compKeys, ...processKeys, ...perfKeys.map((pair) => pair[1])];
    const paretoRows = paretoArr.map((solution: any) => {
      const row: Record<string, string | number> = {};
      compKeys.forEach((key) => {
        row[key] = result.used_composition?.[key] != null ? Number(Number(result.used_composition[key]).toFixed(2)) : "—";
      });
      processKeys.forEach((key) => {
        const value = solution.optimal_process?.[key];
        row[key] = value != null ? Number(Number(value).toFixed(2)) : "—";
      });
      perfKeys.forEach(([key, label]) => {
        const value = solution.predicted_performance?.[key];
        row[label] = value != null ? Number(Number(value).toFixed(2)) : "—";
      });
      return row;
    });
    const paretoWorksheet = XLSX.utils.json_to_sheet(paretoRows, { header: headers });
    paretoWorksheet["!cols"] = headers.map(() => ({ wch: 16 }));
    XLSX.utils.book_append_sheet(workbook, paretoWorksheet, labels.paretoSet);
  }

  const now = new Date();
  const dateStr = `${now.getFullYear()}${String(now.getMonth() + 1).padStart(2, "0")}${String(now.getDate()).padStart(2, "0")}`;
  XLSX.writeFile(workbook, `${labels.filePrefix}_${steelMark}_${dateStr}.xlsx`);
}
