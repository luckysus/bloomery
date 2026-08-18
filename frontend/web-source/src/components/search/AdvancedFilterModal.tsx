import type { KeyboardEvent as ReactKeyboardEvent, RefObject } from "react";
import { ChevronDown, Filter, Search, X } from "lucide-react";

type AdvancedFilterModalProps = {
  open: boolean;
  steelMarkInput: string;
  steelGradeInput: string;
  steelMarkOptions: string[];
  steelGradeOptions: string[];
  filteredSteelMarkOptions: string[];
  filteredSteelGradeOptions: string[];
  steelMarkActiveIndex: number;
  steelGradeActiveIndex: number;
  steelMarkDropdownOpen: boolean;
  steelGradeDropdownOpen: boolean;
  steelMarkRef: RefObject<HTMLDivElement>;
  steelGradeRef: RefObject<HTMLDivElement>;
  steelMarkDropdownListRef: RefObject<HTMLDivElement>;
  steelGradeDropdownListRef: RefObject<HTMLDivElement>;
  onClose: () => void;
  onClear: () => void;
  onApply: () => void;
  onSteelMarkInputChange: (value: string) => void;
  onSteelGradeInputChange: (value: string) => void;
  onSteelMarkFocus: () => void;
  onSteelGradeFocus: () => void;
  onSteelMarkKeyDown: (event: ReactKeyboardEvent<HTMLInputElement>) => void;
  onSteelGradeKeyDown: (event: ReactKeyboardEvent<HTMLInputElement>) => void;
  onToggleSteelMarkDropdown: () => void;
  onToggleSteelGradeDropdown: () => void;
  onSteelMarkSelect: (value: string) => void;
  onSteelGradeSelect: (value: string) => void;
  onSteelMarkPreview: (index: number) => void;
  onSteelGradePreview: (index: number) => void;
};

export default function AdvancedFilterModal({
  open,
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
  onClose,
  onClear,
  onApply,
  onSteelMarkInputChange,
  onSteelGradeInputChange,
  onSteelMarkFocus,
  onSteelGradeFocus,
  onSteelMarkKeyDown,
  onSteelGradeKeyDown,
  onToggleSteelMarkDropdown,
  onToggleSteelGradeDropdown,
  onSteelMarkSelect,
  onSteelGradeSelect,
  onSteelMarkPreview,
  onSteelGradePreview,
}: AdvancedFilterModalProps) {
  if (!open) return null;

  return (
    <div className="fixed inset-0 z-[90] flex items-center justify-center bg-slate-900/40 backdrop-blur-sm fade-in">
      <div
        className="w-auto max-w-[90vw] rounded-2xl border border-slate-200 bg-white p-6 shadow-2xl"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="mb-5 flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Filter size={20} className="text-indigo-600" />
            <h3 className="text-xl font-bold text-slate-900">高级筛选</h3>
          </div>
          <button onClick={onClose} className="text-slate-400 transition-colors hover:text-slate-600">
            <X size={18} />
          </button>
        </div>

        <div className="mb-6 flex gap-4">
          <div className="min-w-[180px] flex-1">
            <label className="mb-1.5 block text-base font-medium text-slate-700">出钢记号</label>
            <div ref={steelMarkRef} className="relative">
              <div className="flex items-center">
                <input
                  type="text"
                  value={steelMarkInput}
                  onChange={(event) => onSteelMarkInputChange(event.target.value)}
                  onFocus={onSteelMarkFocus}
                  onKeyDown={onSteelMarkKeyDown}
                  placeholder="选择或输入"
                  className="w-full rounded-lg border border-slate-200 px-3 py-2 text-base transition-all focus:border-indigo-400 focus:outline-none focus:ring-2 focus:ring-indigo-200"
                />
                <button
                  onClick={onToggleSteelMarkDropdown}
                  className="absolute right-2 text-slate-400 hover:text-slate-600"
                >
                  <ChevronDown
                    size={16}
                    className={`transition-transform ${steelMarkDropdownOpen ? "rotate-180" : ""}`}
                  />
                </button>
              </div>
              {steelMarkDropdownOpen && steelMarkOptions.length > 0 && (
                <div
                  ref={steelMarkDropdownListRef}
                  className="absolute z-20 mt-1 max-h-48 w-full overflow-auto rounded-lg border border-slate-200 bg-white shadow-lg"
                  onMouseDown={(event) => event.stopPropagation()}
                >
                  {filteredSteelMarkOptions.map((option) => (
                    <button
                      key={option}
                      data-active={filteredSteelMarkOptions[steelMarkActiveIndex] === option}
                      onClick={() => onSteelMarkSelect(option)}
                      onMouseEnter={() => onSteelMarkPreview(filteredSteelMarkOptions.indexOf(option))}
                      className={`w-full px-3 py-2 text-left text-base transition-colors hover:bg-indigo-50 hover:text-indigo-700 ${
                        option === steelMarkInput || filteredSteelMarkOptions[steelMarkActiveIndex] === option
                          ? "bg-indigo-50 font-medium text-indigo-700"
                          : "text-slate-700"
                      }`}
                    >
                      {option}
                    </button>
                  ))}
                  {filteredSteelMarkOptions.length === 0 && (
                    <div className="px-3 py-2 text-base text-slate-400">无匹配结果</div>
                  )}
                </div>
              )}
            </div>
          </div>

          <div className="min-w-[180px] flex-1">
            <label className="mb-1.5 block text-base font-medium text-slate-700">钢级代码</label>
            <div ref={steelGradeRef} className="relative">
              <div className="flex items-center">
                <input
                  type="text"
                  value={steelGradeInput}
                  onChange={(event) => onSteelGradeInputChange(event.target.value)}
                  onFocus={onSteelGradeFocus}
                  onKeyDown={onSteelGradeKeyDown}
                  placeholder={steelMarkInput ? "选择或输入" : "请先选择出钢记号"}
                  disabled={!steelMarkInput.trim()}
                  className="w-full rounded-lg border border-slate-200 px-3 py-2 text-base transition-all focus:border-indigo-400 focus:outline-none focus:ring-2 focus:ring-indigo-200 disabled:cursor-not-allowed disabled:bg-slate-50 disabled:text-slate-400"
                />
                <button
                  onClick={() => steelMarkInput.trim() && onToggleSteelGradeDropdown()}
                  className={`absolute right-2 ${
                    steelMarkInput.trim() ? "text-slate-400 hover:text-slate-600" : "cursor-not-allowed text-slate-300"
                  }`}
                >
                  <ChevronDown
                    size={16}
                    className={`transition-transform ${steelGradeDropdownOpen ? "rotate-180" : ""}`}
                  />
                </button>
              </div>
              {steelGradeDropdownOpen && steelGradeOptions.length > 0 && steelMarkInput.trim() && (
                <div
                  ref={steelGradeDropdownListRef}
                  className="absolute z-20 mt-1 max-h-48 w-full overflow-auto rounded-lg border border-slate-200 bg-white shadow-lg"
                  onMouseDown={(event) => event.stopPropagation()}
                >
                  {filteredSteelGradeOptions.map((option) => (
                    <button
                      key={option}
                      data-active={filteredSteelGradeOptions[steelGradeActiveIndex] === option}
                      onClick={() => onSteelGradeSelect(option)}
                      onMouseEnter={() => onSteelGradePreview(filteredSteelGradeOptions.indexOf(option))}
                      className={`w-full px-3 py-2 text-left text-base transition-colors hover:bg-indigo-50 hover:text-indigo-700 ${
                        option === steelGradeInput || filteredSteelGradeOptions[steelGradeActiveIndex] === option
                          ? "bg-indigo-50 font-medium text-indigo-700"
                          : "text-slate-700"
                      }`}
                    >
                      {option}
                    </button>
                  ))}
                  {filteredSteelGradeOptions.length === 0 && (
                    <div className="px-3 py-2 text-base text-slate-400">无匹配结果</div>
                  )}
                </div>
              )}
            </div>
          </div>
        </div>

        <div className="flex items-center justify-between">
          <button onClick={onClear} className="px-4 py-2 text-base text-slate-500 transition-colors hover:text-slate-700">
            清除筛选
          </button>
          <div className="flex items-center gap-2">
            <button
              onClick={onClose}
              className="rounded-lg border border-slate-200 px-4 py-2 text-base text-slate-600 transition-colors hover:bg-slate-50"
            >
              取消
            </button>
            <button
              onClick={onApply}
              className="rounded-lg bg-indigo-600 px-5 py-2 text-base font-medium text-white shadow-sm transition-colors hover:bg-indigo-700"
            >
              <Search size={20} className="-mt-0.5 mr-1.5 inline h-5 w-5" />
              筛选
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
