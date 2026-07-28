import { useCallback, useEffect, useLayoutEffect, useRef, useState, type Dispatch, type RefObject, type SetStateAction } from "react";
import { Database, FileText, Image as ImageIcon, Layers, Microscope, type LucideIcon } from "lucide-react";
import { exportData, getOverview } from "../services/search";
import type { OverviewResponse, SearchResponse, TabId } from "../types/rag";

type NumericInputValue = number | "";

type UseResultWorkspaceArgs = {
  authScopeKey: string;
  data: SearchResponse | null;
  query: string;
  includeProduction: boolean;
  adviceMode: "" | "composition" | "process";
  adviceModeEnabled: boolean;
  resultView: "rag" | "ai";
  isAIMode: boolean;
  activeTab: TabId;
  setActiveTab: Dispatch<SetStateAction<TabId>>;
  resultPaneRef: RefObject<HTMLDivElement>;
  slabWidthMin: number;
  slabWidthMax: number;
  slabThicknessMin: number;
  slabThicknessMax: number;
  yieldRp02Min: number;
  yieldRp02Max: number;
  tensileStrengthMin: number;
  tensileStrengthMax: number;
  elongationMin: number;
  elongationMax: number;
  yieldRp02Value: NumericInputValue;
  tensileStrengthValue: NumericInputValue;
  elongationValue: NumericInputValue;
  steelMark: string;
  steelGrade: string;
  setSlabWidthMin: Dispatch<SetStateAction<number>>;
  setSlabWidthMax: Dispatch<SetStateAction<number>>;
  setSlabThicknessMin: Dispatch<SetStateAction<number>>;
  setSlabThicknessMax: Dispatch<SetStateAction<number>>;
  setYieldRp02Min: Dispatch<SetStateAction<number>>;
  setYieldRp02Max: Dispatch<SetStateAction<number>>;
  setTensileStrengthMin: Dispatch<SetStateAction<number>>;
  setTensileStrengthMax: Dispatch<SetStateAction<number>>;
  setElongationMin: Dispatch<SetStateAction<number>>;
  setElongationMax: Dispatch<SetStateAction<number>>;
};

const resultTabs: Array<{ id: TabId; label: string; icon: LucideIcon }> = [
  { id: "literature", label: "文献片段", icon: FileText },
  { id: "litImages", label: "文献配图", icon: ImageIcon },
  { id: "expImages", label: "实验照片", icon: Microscope },
  { id: "production", label: "生产数据", icon: Database },
];

const initialScrollPositions: Record<TabId, { top: number; left: number }> = {
  literature: { top: 0, left: 0 },
  litImages: { top: 0, left: 0 },
  expImages: { top: 0, left: 0 },
  production: { top: 0, left: 0 },
  standard: { top: 0, left: 0 },
};

export function useResultWorkspace({
  authScopeKey,
  data,
  query,
  includeProduction,
  adviceMode,
  adviceModeEnabled,
  resultView,
  isAIMode,
  activeTab,
  setActiveTab,
  resultPaneRef,
  slabWidthMin,
  slabWidthMax,
  slabThicknessMin,
  slabThicknessMax,
  yieldRp02Min,
  yieldRp02Max,
  tensileStrengthMin,
  tensileStrengthMax,
  elongationMin,
  elongationMax,
  yieldRp02Value,
  tensileStrengthValue,
  elongationValue,
  steelMark,
  steelGrade,
  setSlabWidthMin,
  setSlabWidthMax,
  setSlabThicknessMin,
  setSlabThicknessMax,
  setYieldRp02Min,
  setYieldRp02Max,
  setTensileStrengthMin,
  setTensileStrengthMax,
  setElongationMin,
  setElongationMax,
}: UseResultWorkspaceArgs) {
  const tabScrollPositionsRef = useRef<Record<TabId, { top: number; left: number }>>({ ...initialScrollPositions });
  const [overviewData, setOverviewData] = useState<OverviewResponse | null>(null);
  const [overviewLoading, setOverviewLoading] = useState(false);
  const [exporting, setExporting] = useState(false);

  const persistTabScroll = useCallback((tab: TabId) => {
    const pane = resultPaneRef.current;
    if (!pane) return;
    tabScrollPositionsRef.current[tab] = {
      top: pane.scrollTop,
      left: pane.scrollLeft,
    };
  }, [resultPaneRef]);

  const handleTabChange = useCallback((nextTab: TabId) => {
    if (nextTab === activeTab) return;
    persistTabScroll(activeTab);
    setActiveTab(nextTab);
  }, [activeTab, persistTabScroll, setActiveTab]);

  const handleResultPaneScroll = useCallback(() => {
    const pane = resultPaneRef.current;
    if (!pane) return;
    tabScrollPositionsRef.current[activeTab] = {
      top: pane.scrollTop,
      left: pane.scrollLeft,
    };
  }, [activeTab]);

  const fetchOverview = useCallback(async () => {
    setOverviewLoading(true);
    try {
      const res = await getOverview();
      if (!res.ok) throw new Error(`overview request failed: ${res.status}`);
      const json: OverviewResponse = await res.json();
      setOverviewData(json);
      if (json.slab_width_range) {
        setSlabWidthMin(json.slab_width_range.min_val);
        setSlabWidthMax(json.slab_width_range.max_val);
      }
      if (json.slab_thickness_range) {
        setSlabThicknessMin(json.slab_thickness_range.min_val);
        setSlabThicknessMax(json.slab_thickness_range.max_val);
      }
      if (json.yield_rp02_range) {
        setYieldRp02Min(json.yield_rp02_range.min_val);
        setYieldRp02Max(json.yield_rp02_range.max_val);
      }
      if (json.tensile_strength_range) {
        setTensileStrengthMin(json.tensile_strength_range.min_val);
        setTensileStrengthMax(json.tensile_strength_range.max_val);
      }
      if (json.elongation_range) {
        setElongationMin(json.elongation_range.min_val);
        setElongationMax(json.elongation_range.max_val);
      }
    } catch (err) {
      console.error("获取概览数据失败:", err);
    } finally {
      setOverviewLoading(false);
    }
  }, [
    setElongationMax,
    setElongationMin,
    setSlabThicknessMax,
    setSlabThicknessMin,
    setSlabWidthMax,
    setSlabWidthMin,
    setTensileStrengthMax,
    setTensileStrengthMin,
    setYieldRp02Max,
    setYieldRp02Min,
  ]);

  const handleExport = useCallback(async () => {
    if (!data?.success || exporting) return;

    let yieldMin = yieldRp02Min;
    let yieldMax = yieldRp02Max;
    let tensileMin = tensileStrengthMin;
    let tensileMax = tensileStrengthMax;
    let elongMin = elongationMin;
    let elongMax = elongationMax;

    if (adviceModeEnabled) {
      if (yieldRp02Value !== "" && yieldRp02Value > 0) {
        yieldMin = yieldRp02Value - 1;
        yieldMax = yieldRp02Value + 1;
      } else {
        yieldMin = 0;
        yieldMax = 99999;
      }
      if (tensileStrengthValue !== "" && tensileStrengthValue > 0) {
        tensileMin = tensileStrengthValue - 1;
        tensileMax = tensileStrengthValue + 1;
      } else {
        tensileMin = 0;
        tensileMax = 99999;
      }
      if (elongationValue !== "" && elongationValue > 0) {
        elongMin = Math.round((elongationValue - 1) * 10) / 10;
        elongMax = Math.round((elongationValue + 1) * 10) / 10;
      } else {
        elongMin = 0;
        elongMax = 99999;
      }
    }

    setExporting(true);
    try {
      const params = new URLSearchParams({
        query: query.trim(),
        slab_width_min: slabWidthMin.toString(),
        slab_width_max: slabWidthMax.toString(),
        slab_thickness_min: slabThicknessMin.toString(),
        slab_thickness_max: slabThicknessMax.toString(),
        yield_rp02_min: yieldMin.toString(),
        yield_rp02_max: yieldMax.toString(),
        tensile_strength_min: tensileMin.toString(),
        tensile_strength_max: tensileMax.toString(),
        elongation_min: elongMin.toString(),
        elongation_max: elongMax.toString(),
        steel_mark: steelMark,
        steel_grade: steelGrade,
        advice_mode: adviceMode,
      });

      const res = await exportData(params);
      if (!res.ok) throw new Error("导出失败");

      const blob = await res.blob();
      const url = window.URL.createObjectURL(blob);
      const a = document.createElement("a");
      const contentDisposition = res.headers.get("Content-Disposition") ?? "";
      const filenameMatch = contentDisposition.match(/filename\*?=(?:UTF-8''|"?)([^";]+)/i);
      a.href = url;
      a.download = filenameMatch?.[1]
        ? decodeURIComponent(filenameMatch[1])
        : `export_data_${new Date().toISOString().slice(0, 10)}.xlsx`;
      document.body.appendChild(a);
      a.click();
      window.URL.revokeObjectURL(url);
      document.body.removeChild(a);
    } catch (err) {
      alert("导出失败: " + String(err));
    } finally {
      setExporting(false);
    }
  }, [
    adviceMode,
    adviceModeEnabled,
    data?.success,
    elongationMax,
    elongationMin,
    elongationValue,
    exporting,
    query,
    slabThicknessMax,
    slabThicknessMin,
    slabWidthMax,
    slabWidthMin,
    steelGrade,
    steelMark,
    tensileStrengthMax,
    tensileStrengthMin,
    tensileStrengthValue,
    yieldRp02Max,
    yieldRp02Min,
    yieldRp02Value,
  ]);

  useEffect(() => {
    setOverviewData(null);
    setSlabWidthMin(0);
    setSlabWidthMax(99999);
    setSlabThicknessMin(0);
    setSlabThicknessMax(99999);
    setYieldRp02Min(0);
    setYieldRp02Max(99999);
    setTensileStrengthMin(0);
    setTensileStrengthMax(99999);
    setElongationMin(0);
    setElongationMax(99999);
    void fetchOverview();
  }, [authScopeKey, fetchOverview]);

  useEffect(() => {
    if (!includeProduction && (activeTab === "production" || activeTab === "standard")) {
      setActiveTab("literature");
    }
  }, [includeProduction, activeTab, setActiveTab]);

  const productionRecords = Array.isArray(data?.production_records) ? data.production_records : [];
  const adviceStandardColumns = Array.isArray(data?.advice_standard_columns) ? data.advice_standard_columns : [];
  const adviceStandardRecords = Array.isArray(data?.advice_standard_records) ? data.advice_standard_records : [];
  const hasProductionData = !!(data?.success && productionRecords.length > 0);
  const hasStandardData = !!(data?.advice_mode && adviceStandardColumns.length > 0 && adviceStandardRecords.length > 0);
  const isPureProductionSearch = !query.trim() && includeProduction && hasProductionData;
  const baseTabs = isPureProductionSearch
    ? resultTabs.filter((tab) => tab.id === "production")
    : includeProduction && hasProductionData
      ? resultTabs
      : resultTabs.filter((tab) => tab.id !== "production");
  const visibleTabs: Array<{ id: TabId; label: string; icon: LucideIcon }> = adviceModeEnabled
    ? [
        {
          id: "production",
          label: "生产数据",
          icon: Database,
        },
        ...(hasStandardData
          ? [
              {
                id: "standard" as const,
                label: "成分标准",
                icon: Layers,
              },
            ]
          : []),
      ]
    : hasStandardData
      ? [
          ...baseTabs,
          {
            id: "standard" as const,
            label: "成分标准",
            icon: Layers,
          },
        ]
      : baseTabs;
  const totalProductionCount = productionRecords.length;
  const displayedProductionRecords = totalProductionCount > 50
    ? productionRecords.slice(0, 50)
    : productionRecords;

  useEffect(() => {
    if (visibleTabs.length === 0) return;
    if (!visibleTabs.some((tab) => tab.id === activeTab)) {
      setActiveTab(visibleTabs[0].id);
    }
  }, [activeTab, setActiveTab, visibleTabs]);

  useEffect(() => {
    tabScrollPositionsRef.current = { ...initialScrollPositions };
    const pane = resultPaneRef.current;
    if (pane) {
      pane.scrollTop = 0;
      pane.scrollLeft = 0;
    }
  }, [data]);

  useLayoutEffect(() => {
    const pane = resultPaneRef.current;
    if (!pane) return;

    const savedPosition = tabScrollPositionsRef.current[activeTab] ?? { top: 0, left: 0 };
    pane.scrollTop = savedPosition.top;
    pane.scrollLeft = savedPosition.left;
  }, [activeTab, resultView, isAIMode]);

  return {
    overviewData,
    overviewLoading,
    exporting,
    visibleTabs,
    totalProductionCount,
    displayedProductionRecords,
    handleTabChange,
    handleResultPaneScroll,
    handleExport,
  };
}
