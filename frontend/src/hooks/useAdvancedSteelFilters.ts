import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react";
import type { SearchResponse } from "../types/rag";

const STEEL_MARK_FIELD = "\u51fa\u94a2\u8bb0\u53f7";
const STEEL_GRADE_FIELD = "\u94a2\u7ea7\u4ee3\u7801";

type UseAdvancedSteelFiltersOptions = {
  data: SearchResponse | null;
  onApply?: () => void;
};

export function useAdvancedSteelFilters({ data, onApply }: UseAdvancedSteelFiltersOptions) {
  const [showAdvancedFilter, setShowAdvancedFilter] = useState(false);
  const [steelMark, setSteelMark] = useState("");
  const [steelGrade, setSteelGrade] = useState("");
  const [steelMarkInput, setSteelMarkInput] = useState("");
  const [steelGradeInput, setSteelGradeInput] = useState("");
  const [steelMarkSearchTerm, setSteelMarkSearchTerm] = useState("");
  const [steelGradeSearchTerm, setSteelGradeSearchTerm] = useState("");
  const [steelMarkActiveIndex, setSteelMarkActiveIndex] = useState(-1);
  const [steelGradeActiveIndex, setSteelGradeActiveIndex] = useState(-1);
  const [steelMarkDropdownOpen, setSteelMarkDropdownOpen] = useState(false);
  const [steelGradeDropdownOpen, setSteelGradeDropdownOpen] = useState(false);
  const steelMarkRef = useRef<HTMLDivElement>(null);
  const steelGradeRef = useRef<HTMLDivElement>(null);
  const steelMarkDropdownListRef = useRef<HTMLDivElement>(null);
  const steelGradeDropdownListRef = useRef<HTMLDivElement>(null);

  const steelMarkOptions = useMemo(() => {
    if (!data?.production_records?.length) return [];
    const marks = new Set<string>();
    data.production_records.forEach((row) => {
      const mark = row[STEEL_MARK_FIELD];
      if (mark && typeof mark === "string" && mark.trim()) {
        marks.add(mark.trim());
      }
    });
    return Array.from(marks).sort();
  }, [data?.production_records]);

  const steelGradeOptions = useMemo(() => {
    if (!data?.production_records?.length || !steelMarkInput) return [];
    const grades = new Set<string>();
    data.production_records.forEach((row) => {
      if (row[STEEL_MARK_FIELD] === steelMarkInput) {
        const grade = row[STEEL_GRADE_FIELD];
        if (grade && typeof grade === "string" && grade.trim()) {
          grades.add(grade.trim());
        }
      }
    });
    return Array.from(grades).sort();
  }, [data?.production_records, steelMarkInput]);

  const filteredSteelMarkOptions = steelMarkOptions.filter(
    (option) => !steelMarkSearchTerm || option.toLowerCase().includes(steelMarkSearchTerm.toLowerCase())
  );
  const filteredSteelGradeOptions = steelGradeOptions.filter(
    (option) => !steelGradeSearchTerm || option.toLowerCase().includes(steelGradeSearchTerm.toLowerCase())
  );

  const openAdvancedFilter = () => {
    setSteelMarkInput(steelMark);
    setSteelGradeInput(steelGrade);
    setSteelMarkSearchTerm("");
    setSteelGradeSearchTerm("");
    setSteelMarkActiveIndex(-1);
    setSteelGradeActiveIndex(-1);
    setSteelMarkDropdownOpen(false);
    setSteelGradeDropdownOpen(false);
    setShowAdvancedFilter(true);
  };

  const handleSteelMarkSelect = (mark: string) => {
    setSteelMarkInput(mark);
    setSteelMarkSearchTerm("");
    setSteelMarkActiveIndex(-1);
    setSteelMarkDropdownOpen(false);

    const grades = new Set<string>();
    if (data?.production_records?.length) {
      data.production_records.forEach((row) => {
        if (row[STEEL_MARK_FIELD] === mark) {
          const grade = row[STEEL_GRADE_FIELD];
          if (grade && typeof grade === "string" && grade.trim()) {
            grades.add(grade.trim());
          }
        }
      });
    }
    const gradeList = Array.from(grades).sort();

    setSteelGradeInput(gradeList.length > 0 ? gradeList[0] : "");
    setSteelGradeSearchTerm("");
    setSteelGradeActiveIndex(-1);
    setSteelGradeDropdownOpen(gradeList.length > 1);
  };

  const handleSteelGradeSelect = (grade: string) => {
    setSteelGradeInput(grade);
    setSteelGradeSearchTerm("");
    setSteelGradeActiveIndex(-1);
    setSteelGradeDropdownOpen(false);
  };

  const handleSteelMarkInputChange = (nextValue: string) => {
    setSteelMarkInput(nextValue);
    setSteelMarkSearchTerm(nextValue);
    setSteelMarkActiveIndex(-1);
    setSteelMarkDropdownOpen(true);
    setSteelGradeInput("");
    setSteelGradeSearchTerm("");
    setSteelGradeActiveIndex(-1);
  };

  const handleSteelGradeInputChange = (nextValue: string) => {
    setSteelGradeInput(nextValue);
    setSteelGradeSearchTerm(nextValue);
    setSteelGradeActiveIndex(-1);
    setSteelGradeDropdownOpen(true);
  };

  const handleSteelMarkFocus = () => {
    setSteelMarkSearchTerm("");
    setSteelMarkActiveIndex(-1);
    setSteelMarkDropdownOpen(true);
  };

  const handleSteelGradeFocus = () => {
    if (!steelMarkInput.trim()) return;
    setSteelGradeSearchTerm("");
    setSteelGradeActiveIndex(-1);
    setSteelGradeDropdownOpen(true);
  };

  const toggleSteelMarkDropdown = () => {
    const nextOpen = !steelMarkDropdownOpen;
    if (nextOpen) {
      setSteelMarkSearchTerm("");
    }
    setSteelMarkActiveIndex(-1);
    setSteelMarkDropdownOpen(nextOpen);
  };

  const toggleSteelGradeDropdown = () => {
    const nextOpen = !steelGradeDropdownOpen;
    if (nextOpen) {
      setSteelGradeSearchTerm("");
    }
    setSteelGradeActiveIndex(-1);
    setSteelGradeDropdownOpen(nextOpen);
  };

  const previewSteelMarkOption = (index: number) => {
    if (filteredSteelMarkOptions.length === 0) return;
    const nextIndex = ((index % filteredSteelMarkOptions.length) + filteredSteelMarkOptions.length) % filteredSteelMarkOptions.length;
    setSteelMarkActiveIndex(nextIndex);
    setSteelMarkInput(filteredSteelMarkOptions[nextIndex]);
  };

  const previewSteelGradeOption = (index: number) => {
    if (filteredSteelGradeOptions.length === 0) return;
    const nextIndex = ((index % filteredSteelGradeOptions.length) + filteredSteelGradeOptions.length) % filteredSteelGradeOptions.length;
    setSteelGradeActiveIndex(nextIndex);
    setSteelGradeInput(filteredSteelGradeOptions[nextIndex]);
  };

  const handleSteelMarkInputKeyDown = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      if (filteredSteelMarkOptions.length === 0) {
        setSteelMarkDropdownOpen(true);
        setSteelMarkActiveIndex(-1);
        return;
      }
      if (!steelMarkDropdownOpen) {
        setSteelMarkDropdownOpen(true);
        previewSteelMarkOption(0);
        return;
      }
      previewSteelMarkOption(steelMarkActiveIndex + 1);
      return;
    }

    if (event.key === "ArrowUp") {
      event.preventDefault();
      if (filteredSteelMarkOptions.length === 0) {
        setSteelMarkDropdownOpen(true);
        setSteelMarkActiveIndex(-1);
        return;
      }
      if (!steelMarkDropdownOpen) {
        setSteelMarkDropdownOpen(true);
        previewSteelMarkOption(filteredSteelMarkOptions.length - 1);
        return;
      }
      previewSteelMarkOption(steelMarkActiveIndex - 1);
      return;
    }

    if (event.key === "Enter" && steelMarkDropdownOpen && filteredSteelMarkOptions.length > 0) {
      event.preventDefault();
      const targetIndex = steelMarkActiveIndex >= 0 ? steelMarkActiveIndex : 0;
      handleSteelMarkSelect(filteredSteelMarkOptions[targetIndex]);
      return;
    }

    if (event.key === "Escape") {
      setSteelMarkDropdownOpen(false);
      setSteelMarkActiveIndex(-1);
    }
  };

  const handleSteelGradeInputKeyDown = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    if (!steelMarkInput.trim()) return;

    if (event.key === "ArrowDown") {
      event.preventDefault();
      if (filteredSteelGradeOptions.length === 0) {
        setSteelGradeDropdownOpen(true);
        setSteelGradeActiveIndex(-1);
        return;
      }
      if (!steelGradeDropdownOpen) {
        setSteelGradeDropdownOpen(true);
        previewSteelGradeOption(0);
        return;
      }
      previewSteelGradeOption(steelGradeActiveIndex + 1);
      return;
    }

    if (event.key === "ArrowUp") {
      event.preventDefault();
      if (filteredSteelGradeOptions.length === 0) {
        setSteelGradeDropdownOpen(true);
        setSteelGradeActiveIndex(-1);
        return;
      }
      if (!steelGradeDropdownOpen) {
        setSteelGradeDropdownOpen(true);
        previewSteelGradeOption(filteredSteelGradeOptions.length - 1);
        return;
      }
      previewSteelGradeOption(steelGradeActiveIndex - 1);
      return;
    }

    if (event.key === "Enter" && steelGradeDropdownOpen && filteredSteelGradeOptions.length > 0) {
      event.preventDefault();
      const targetIndex = steelGradeActiveIndex >= 0 ? steelGradeActiveIndex : 0;
      handleSteelGradeSelect(filteredSteelGradeOptions[targetIndex]);
      return;
    }

    if (event.key === "Escape") {
      setSteelGradeDropdownOpen(false);
      setSteelGradeActiveIndex(-1);
    }
  };

  const handleAdvancedFilterApply = () => {
    const markVal = steelMarkInput.trim();
    const gradeVal = steelGradeInput.trim();

    if (markVal && steelMarkOptions.length > 0 && !steelMarkOptions.includes(markVal)) {
      alert(`\u51fa\u94a2\u8bb0\u53f7 "${markVal}" \u4e0d\u5b58\u5728\uff0c\u8bf7\u4ece\u4e0b\u62c9\u5217\u8868\u4e2d\u9009\u62e9\u6216\u8f93\u5165\u6709\u6548\u503c`);
      return;
    }
    if (gradeVal && steelGradeOptions.length > 0 && !steelGradeOptions.includes(gradeVal)) {
      alert(`\u94a2\u7ea7\u4ee3\u7801 "${gradeVal}" \u5728\u5f53\u524d\u51fa\u94a2\u8bb0\u53f7\u4e0b\u4e0d\u5b58\u5728\uff0c\u8bf7\u4ece\u4e0b\u62c9\u5217\u8868\u4e2d\u9009\u62e9\u6216\u8f93\u5165\u6709\u6548\u503c`);
      return;
    }
    setSteelMark(markVal);
    setSteelGrade(gradeVal);
    setShowAdvancedFilter(false);
    onApply?.();
  };

  const handleAdvancedFilterClear = () => {
    setSteelMarkInput("");
    setSteelGradeInput("");
    setSteelMarkSearchTerm("");
    setSteelGradeSearchTerm("");
    setSteelMarkActiveIndex(-1);
    setSteelGradeActiveIndex(-1);
  };

  useEffect(() => {
    const handler = (event: MouseEvent) => {
      if (steelMarkRef.current && !steelMarkRef.current.contains(event.target as Node)) {
        setSteelMarkDropdownOpen(false);
        setSteelMarkActiveIndex(-1);
      }
      if (steelGradeRef.current && !steelGradeRef.current.contains(event.target as Node)) {
        setSteelGradeDropdownOpen(false);
        setSteelGradeActiveIndex(-1);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, []);

  useEffect(() => {
    if (!steelMarkDropdownOpen || filteredSteelMarkOptions.length === 0) {
      setSteelMarkActiveIndex(-1);
      return;
    }
    setSteelMarkActiveIndex((prev) =>
      prev >= filteredSteelMarkOptions.length ? filteredSteelMarkOptions.length - 1 : prev
    );
  }, [steelMarkDropdownOpen, filteredSteelMarkOptions]);

  useEffect(() => {
    if (!steelGradeDropdownOpen || filteredSteelGradeOptions.length === 0) {
      setSteelGradeActiveIndex(-1);
      return;
    }
    setSteelGradeActiveIndex((prev) =>
      prev >= filteredSteelGradeOptions.length ? filteredSteelGradeOptions.length - 1 : prev
    );
  }, [steelGradeDropdownOpen, filteredSteelGradeOptions]);

  useEffect(() => {
    if (!steelMarkDropdownOpen || steelMarkActiveIndex < 0) return;
    const activeItem = steelMarkDropdownListRef.current?.querySelector<HTMLElement>('[data-active="true"]');
    activeItem?.scrollIntoView({ block: "nearest" });
  }, [steelMarkDropdownOpen, steelMarkActiveIndex]);

  useEffect(() => {
    if (!steelGradeDropdownOpen || steelGradeActiveIndex < 0) return;
    const activeItem = steelGradeDropdownListRef.current?.querySelector<HTMLElement>('[data-active="true"]');
    activeItem?.scrollIntoView({ block: "nearest" });
  }, [steelGradeDropdownOpen, steelGradeActiveIndex]);

  return {
    showAdvancedFilter,
    setShowAdvancedFilter,
    steelMark,
    steelGrade,
    steelMarkInput,
    steelGradeInput,
    steelMarkOptions,
    steelGradeOptions,
    filteredSteelMarkOptions,
    filteredSteelGradeOptions,
    steelMarkActiveIndex,
    steelGradeActiveIndex,
    steelMarkDropdownOpen,
    steelGradeDropdownOpen,
    steelMarkRef,
    steelGradeRef,
    steelMarkDropdownListRef,
    steelGradeDropdownListRef,
    openAdvancedFilter,
    handleAdvancedFilterClear,
    handleAdvancedFilterApply,
    handleSteelMarkInputChange,
    handleSteelGradeInputChange,
    handleSteelMarkFocus,
    handleSteelGradeFocus,
    handleSteelMarkInputKeyDown,
    handleSteelGradeInputKeyDown,
    toggleSteelMarkDropdown,
    toggleSteelGradeDropdown,
    handleSteelMarkSelect,
    handleSteelGradeSelect,
    previewSteelMarkOption,
    previewSteelGradeOption,
  };
}
