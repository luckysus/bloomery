import { Calculator, CheckCircle2, LoaderCircle, TriangleAlert } from "lucide-react";
import { useState, type FormEvent } from "react";
import {
  desktop,
  type CarbonEquivalentFormula,
  type CarbonEquivalentResult,
  type CompositionUnit,
} from "../../bridge/desktop";
import { useLocale } from "../../i18n/locale";
import DatasetPanel from "./DatasetPanel";
import OnnxInferencePanel from "./OnnxInferencePanel";

const elements = ["C", "Mn", "Cr", "Mo", "V", "Ni", "Cu", "Si", "B"] as const;
type Element = (typeof elements)[number];
type Composition = Record<Element, string>;

const initialComposition: Composition = {
  C: "0.20",
  Mn: "1.00",
  Cr: "0.25",
  Mo: "0.05",
  V: "0.02",
  Ni: "0.20",
  Cu: "0.30",
  Si: "0.20",
  B: "0.001",
};

function parseComposition(values: Composition) {
  const composition: Record<string, number> = {};
  for (const [element, raw] of Object.entries(values)) {
    if (!raw.trim()) continue;
    const value = Number(raw);
    if (!Number.isFinite(value)) return null;
    composition[element] = value;
  }
  return composition;
}

export default function AnalysisPage() {
  const { t } = useLocale();
  const [formula, setFormula] = useState<CarbonEquivalentFormula>("iiw");
  const [unit, setUnit] = useState<CompositionUnit>("percent_mass");
  const [composition, setComposition] = useState<Composition>(initialComposition);
  const [result, setResult] = useState<CarbonEquivalentResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const updateElement = (element: Element, value: string) => {
    setComposition((current) => ({ ...current, [element]: value }));
    setResult(null);
    setError(null);
  };

  const calculate = async (event: FormEvent) => {
    event.preventDefault();
    const parsed = parseComposition(composition);
    if (!parsed) {
      setError(t("analysisInvalidValue"));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      setResult(await desktop.calculateSteelCarbonEquivalent({ formula, unit, composition: parsed }));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t("analysisInvalidValue"));
      setResult(null);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="bloomery-analysis" data-testid="analysis-page">
      <header className="bloomery-analysis-header">
        <div>
          <p className="bloomery-eyebrow">STEEL DOMAIN / ANALYSIS</p>
          <h1>{t("analysisTitle")}</h1>
          <p>{t("analysisLede")}</p>
        </div>
        <div className="bloomery-analysis-mark" aria-hidden="true"><Calculator size={24} /></div>
      </header>

      <div className="bloomery-analysis-grid">
        <section className="bloomery-analysis-tool" aria-labelledby="carbon-equivalent-heading">
          <div className="bloomery-section-heading">
            <div>
              <p className="bloomery-eyebrow">CALC-01</p>
              <h2 id="carbon-equivalent-heading">{t("analysisCalculatorTitle")}</h2>
            </div>
            <span className="bloomery-analysis-source">RUST / LOCAL</span>
          </div>
          <p className="bloomery-analysis-copy">{t("analysisCalculatorCopy")}</p>

          <form onSubmit={calculate}>
            <fieldset className="bloomery-analysis-fieldset">
              <legend>{t("analysisFormula")}</legend>
              <div className="bloomery-segmented-control" role="group" aria-label={t("analysisFormula")}>
                {(["iiw", "pcm"] as const).map((value) => (
                  <button
                    key={value}
                    type="button"
                    aria-pressed={formula === value}
                    className={formula === value ? "is-selected" : ""}
                    onClick={() => { setFormula(value); setResult(null); }}
                  >
                    {t(value === "iiw" ? "analysisIiw" : "analysisPcm")}
                  </button>
                ))}
              </div>
            </fieldset>

            <label className="bloomery-analysis-select-label" htmlFor="composition-unit">{t("analysisUnit")}</label>
            <select id="composition-unit" value={unit} onChange={(event) => { setUnit(event.target.value as CompositionUnit); setResult(null); }}>
              <option value="percent_mass">{t("analysisPercentMass")}</option>
              <option value="mass_fraction">{t("analysisMassFraction")}</option>
            </select>

            <div className="bloomery-composition-grid">
              {elements.map((element) => (
                <label key={element} htmlFor={`composition-${element}`}>
                  <span>{element}</span>
                  <input
                    id={`composition-${element}`}
                    inputMode="decimal"
                    step="any"
                    type="number"
                    value={composition[element]}
                    onChange={(event) => updateElement(element, event.target.value)}
                  />
                </label>
              ))}
            </div>

            <button className="bloomery-analysis-submit" type="submit" disabled={busy}>
              {busy ? <LoaderCircle className="bloomery-spin" size={17} aria-hidden="true" /> : <Calculator size={17} aria-hidden="true" />}
              <span>{busy ? t("analysisCalculating") : t("analysisCalculate")}</span>
            </button>
          </form>
          {error && <p className="bloomery-analysis-error" role="alert"><TriangleAlert size={16} aria-hidden="true" />{error}</p>}
        </section>

        <section className="bloomery-analysis-result" aria-labelledby="analysis-result-heading">
          <div className="bloomery-section-heading">
            <div>
              <p className="bloomery-eyebrow">AUDIT OUTPUT</p>
              <h2 id="analysis-result-heading">{t("analysisResult")}</h2>
            </div>
            {result && <CheckCircle2 size={19} className="bloomery-analysis-success" aria-label="completed" />}
          </div>
          {result ? (
            <div className="bloomery-result-body">
              <output id="carbon-equivalent-value" data-testid="carbon-equivalent-value">{result.value.toFixed(4)}</output>
              <span className="bloomery-result-unit">CE / %</span>
              <dl>
                <div><dt>{t("analysisFormulaId")}</dt><dd>{result.formula_id}</dd></div>
                <div><dt>{t("analysisFormula")}</dt><dd>{result.expression}</dd></div>
              </dl>
              <p className="bloomery-analysis-note"><strong>{t("analysisApplicability")}</strong>{result.applicability_note}</p>
            </div>
          ) : (
            <div className="bloomery-result-empty">
              <Calculator size={22} aria-hidden="true" />
              <span>{t("analysisCalculate")}</span>
            </div>
          )}
        </section>
      </div>

      <DatasetPanel />
      <OnnxInferencePanel />
    </div>
  );
}
