import { useState } from "react";
import { CheckCircle2, Eraser, Loader2 } from "lucide-react";
import { desktop, type SteelCleaningPlan, type SteelCleaningSummary } from "../../bridge/desktop";
import { Checkbox } from "../../components/ui/checkbox";
import { Input } from "../../components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../../components/ui/select";

/** 第 31 章数据清洗：重复行、缺失值、异常值处理，结果落盘为新数据集。 */
export default function DataLabCleaningPanel({
  datasetId,
  onCleaned,
}: {
  datasetId: string;
  onCleaned: (message: string) => void;
}) {
  const [dropDuplicates, setDropDuplicates] = useState(true);
  const [missingStrategy, setMissingStrategy] = useState<SteelCleaningPlan["missingStrategy"]>("drop_rows");
  const [outlierStrategy, setOutlierStrategy] = useState<SteelCleaningPlan["outlierStrategy"]>("keep");
  const [multiplier, setMultiplier] = useState("1.5");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [summary, setSummary] = useState<SteelCleaningSummary | null>(null);
  const [cleanedName, setCleanedName] = useState("");

  const run = async () => {
    if (!datasetId) {
      setError("请先选择数据集");
      return;
    }
    setBusy(true);
    setError("");
    try {
      const result = await desktop.cleanSteelDataset({
        datasetId,
        plan: {
          dropDuplicateRows: dropDuplicates,
          missingStrategy,
          outlierStrategy,
          outlierIqrMultiplier: Number(multiplier) || 1.5,
          columns: [],
        },
      });
      setSummary(result.summary);
      setCleanedName(result.dataset.sourceName);
      onCleaned(`已生成清洗后的数据集：${result.dataset.sourceName}`);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "数据清洗失败");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="suna-data-chart">
      <h3>数据清洗</h3>
      <div className="suna-data-cleaning">
        <label className="suna-data-cleaning-toggle">
          <Checkbox
            aria-label="删除重复行"
            checked={dropDuplicates}
            onCheckedChange={(checked) => setDropDuplicates(checked === true)}
          />
          删除重复行
        </label>
        <label>
          缺失值
          <Select value={missingStrategy} onValueChange={(value) => setMissingStrategy(value as SteelCleaningPlan["missingStrategy"])}>
            <SelectTrigger aria-label="缺失值处理"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="keep">保留</SelectItem>
              <SelectItem value="drop_rows">删除含缺失值的行</SelectItem>
              <SelectItem value="fill_mean">用列均值填充</SelectItem>
            </SelectContent>
          </Select>
        </label>
        <label>
          异常值
          <Select value={outlierStrategy} onValueChange={(value) => setOutlierStrategy(value as SteelCleaningPlan["outlierStrategy"])}>
            <SelectTrigger aria-label="异常值处理"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="keep">保留</SelectItem>
              <SelectItem value="drop_rows">按 IQR 剔除整行</SelectItem>
              <SelectItem value="clip">按 IQR 截断到边界</SelectItem>
            </SelectContent>
          </Select>
        </label>
        <label>
          IQR 倍数
          <Input aria-label="IQR 倍数" type="number" min="0" step="0.5" value={multiplier} onChange={(event) => setMultiplier(event.target.value)} />
        </label>
        <button className="suna-primary-button" onClick={() => void run()} disabled={busy}>
          {busy ? <Loader2 className="suna-spin" size={15} /> : <Eraser size={15} />}
          {busy ? "清洗中..." : "执行清洗"}
        </button>
      </div>
      {error && <p className="suna-data-cleaning-error" role="alert">{error}</p>}
      {summary && (
        <div className="suna-data-cleaning-summary" role="status">
          <span><CheckCircle2 size={13} />{cleanedName}</span>
          <span>行数 {summary.rowCountBefore} → {summary.rowCountAfter}</span>
          <span>重复行 {summary.duplicateRows}（删除 {summary.duplicateRowsRemoved}）</span>
          <span>缺失单元格 {summary.missingCells}（删行 {summary.missingRowsRemoved} / 填充 {summary.filledCells}）</span>
          <span>异常单元格 {summary.outlierCells}（删行 {summary.outlierRowsRemoved} / 截断 {summary.clippedCells}）</span>
        </div>
      )}
    </div>
  );
}
