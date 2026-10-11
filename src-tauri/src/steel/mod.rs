mod agent_tools;
mod analysis;
mod calculators;
mod cleaning;
mod datasets;
mod evaluations;
mod explain;
mod multivariate;
mod optimization_tool;
mod series;
mod tool;

pub use agent_tools::{SteelAgentGateway, SteelAgentGatewayFuture};
pub use analysis::{
    analyze_dataset, DatasetAnalysis, DatasetAnalysisRequest, DatasetColumnAnalysis,
    DatasetCorrelation, DatasetDistributionBin, DatasetGroupColumnSummary, DatasetGroupSummary,
    DatasetValueFrequency,
};
pub use calculators::{
    calculate_carbon_equivalent, CarbonEquivalentFormula, CarbonEquivalentResult, CompositionInput,
    CompositionUnit, SteelCalculationError,
};
pub use cleaning::{clean_dataset, to_csv, DatasetCleaningPlan, DatasetCleaningSummary};
pub use datasets::{
    hash_dataset_source, preview_dataset, read_dataset_table, DatasetColumnPreview, DatasetPreview,
    DatasetPreviewRequest, DatasetTable,
};
pub use evaluations::{parse_suite, run_rust_categories, CategoryReport, EvaluationReport};
pub use explain::{explain_linear, parse_linear_artifact, LinearModel, ShapExplanation, ShapValue};
pub use multivariate::{
    multivariate_analysis, KMeansResult, MultivariateRequest, MultivariateResult, PcaComponent,
    PcaResult,
};
pub use optimization_tool::{
    optimization_status_tool, optimize_constrained_tool, OptimizationGateway,
};
pub use series::{
    dataset_series, numeric_matrix, DatasetSeries, DatasetSeriesColumn, DatasetSeriesRequest,
};
pub use tool::{carbon_equivalent_tool, SteelToolExecutor};
