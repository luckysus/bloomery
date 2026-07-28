import * as XLSX from "xlsx";

interface ExportOptimizationSchemeOptions {
  steelMark: string;
  result: any;
  displayProcess: Record<string, any> | null | undefined;
  displayPerf: Record<string, number> | null | undefined;
}

const PERF_MAP: Record<string, { label: string; unit: string }> = {
  yield_strength: { label: "屈服强度", unit: "MPa" },
  tensile_strength: { label: "抗拉强度", unit: "MPa" },
  elongation: { label: "延伸率", unit: "%" },
};

export function exportOptimizationScheme({ steelMark, result, displayProcess, displayPerf }: ExportOptimizationSchemeOptions) {
  const rows: { 参数: string; 值: string }[] = [];
  if (result.used_composition) {
    Object.entries(result.used_composition).forEach(([key, value]: [string, any]) => {
      rows.push({ 参数: key, 值: value != null ? String(value) : "—" });
    });
  }
  if (displayProcess) {
    Object.entries(displayProcess).forEach(([key, value]: [string, any]) => {
      rows.push({ 参数: key, 值: typeof value === "number" ? value.toFixed(2) : String(value ?? "—") });
    });
  }
  if (displayPerf) {
    Object.entries(PERF_MAP).forEach(([key, { label, unit }]) => {
      const value = displayPerf?.[key];
      rows.push({ 参数: label, 值: value != null ? `${value.toFixed(2)} ${unit}` : "—" });
    });
  }

  const worksheet = XLSX.utils.json_to_sheet(rows);
  worksheet["!cols"] = [{ wch: 16 }, { wch: 20 }];
  const workbook = XLSX.utils.book_new();
  XLSX.utils.book_append_sheet(workbook, worksheet, "最优方案");

  if (result.pareto_front && result.pareto_front.length > 0) {
    const paretoArr = result.pareto_front as Array<{
      predicted_performance: Record<string, number>;
      optimal_process: Record<string, number>;
    }>;
    const compKeys = result.used_composition ? Object.keys(result.used_composition) : [];
    const processKeys = paretoArr[0]?.optimal_process ? Object.keys(paretoArr[0].optimal_process) : [];
    const perfKeys = [
      ["yield_strength", "屈服强度(MPa)"],
      ["tensile_strength", "抗拉强度(MPa)"],
      ["elongation", "延伸率(%)"],
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
    XLSX.utils.book_append_sheet(workbook, paretoWorksheet, "帕累托解集");
  }

  const now = new Date();
  const dateStr = `${now.getFullYear()}${String(now.getMonth() + 1).padStart(2, "0")}${String(now.getDate()).padStart(2, "0")}`;
  XLSX.writeFile(workbook, `优化方案_${steelMark}_${dateStr}.xlsx`);
}
