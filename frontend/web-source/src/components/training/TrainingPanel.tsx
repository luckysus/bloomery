import React, { type Dispatch, type SetStateAction } from "react";
import {
  Brain,
  CheckCircle,
  FileText,
  Loader2,
  Play,
  RefreshCw,
  Settings,
  Square,
  Trash2,
  X,
} from "lucide-react";

type TrainingRunStatus = "idle" | "running" | "completed" | "failed" | "cancelled";

type TrainingModel = {
  version: string;
  is_active?: boolean;
  sample_count?: number | null;
};

interface TrainingPanelProps {
  showTraining: boolean;
  setShowTraining: Dispatch<SetStateAction<boolean>>;
  setShowOptimizer: Dispatch<SetStateAction<boolean>>;
  trainingEntrySource: "main" | "optimizer";
  setTrainingEntrySource: Dispatch<SetStateAction<"main" | "optimizer">>;
  trainingTab: "train" | "models";
  setTrainingTab: Dispatch<SetStateAction<"train" | "models">>;
  trainingModelVersion: string;
  setTrainingModelVersion: Dispatch<SetStateAction<string>>;
  trainingModelType: 'catboost' | 'pinn';
  setTrainingModelType: Dispatch<SetStateAction<'catboost' | 'pinn'>>;
  maxRows: string;
  setMaxRows: Dispatch<SetStateAction<string>>;
  handleStartTraining: () => void | Promise<void>;
  trainingStarting: boolean;
  trainingRunStatus: TrainingRunStatus;
  handleCancelTraining: () => void | Promise<void>;
  cancelling: boolean;
  trainingStatus: any;
  trainingLogs: string[];
  trainingLogEndRef: React.Ref<HTMLDivElement>;
  fetchTrainingModels: () => void | Promise<void>;
  trainingModelsLoading: boolean;
  trainingModels: TrainingModel[];
  expandedModelLogs: Set<string>;
  modelLogDataMap: Record<string, any>;
  handleActivateModel: (version: string) => void | Promise<void>;
  handleViewModelLogs: (version: string) => void | Promise<void>;
  handleDeleteModel: (version: string) => void | Promise<void>;
  modelLogLoading: string | null;
  deletingModel: string | null;
}

export default function TrainingPanel({
  showTraining,
  setShowTraining,
  setShowOptimizer,
  trainingEntrySource,
  setTrainingEntrySource,
  trainingTab,
  setTrainingTab,
  trainingModelVersion,
  setTrainingModelVersion,
  trainingModelType,
  setTrainingModelType,
  maxRows,
  setMaxRows,
  handleStartTraining,
  trainingStarting,
  trainingRunStatus,
  handleCancelTraining,
  cancelling,
  trainingStatus,
  trainingLogs,
  trainingLogEndRef,
  fetchTrainingModels,
  trainingModelsLoading,
  trainingModels,
  expandedModelLogs,
  modelLogDataMap,
  handleActivateModel,
  handleViewModelLogs,
  handleDeleteModel,
  modelLogLoading,
  deletingModel,
}: TrainingPanelProps) {
  return (
    <>
      {showTraining && (
        <div className="fixed inset-0 z-50 bg-white flex flex-col">
          {/* 顶部栏 */}
          <div className="flex items-center justify-between px-6 py-4 border-b border-slate-200 shrink-0 max-md:px-3 max-md:py-3">
            <h2 className="text-xl font-bold text-slate-900 flex items-center gap-2 max-md:text-base">
              <Brain className="w-6 h-6 text-indigo-600 shrink-0 max-md:w-5 max-md:h-5" />
              <span className="whitespace-nowrap">模型训练管理</span>
            </h2>
            <button onClick={() => { setShowTraining(false); if (trainingEntrySource === 'optimizer') { setShowOptimizer(true); } setTrainingEntrySource('main'); }} className="p-2 rounded-lg hover:bg-slate-100 text-slate-500 hover:text-slate-700 transition-colors shrink-0">
              <X className="w-6 h-6" />
            </button>
          </div>

          {/* Tab 切换 */}
          <div className="flex border-b border-slate-200 px-6 shrink-0 max-md:px-3">
            {([['train', '开始训练', Play], ['models', '模型管理', Settings]] as const).map(([key, label, Icon]) => (
              <button key={key} onClick={() => setTrainingTab(key as any)}
                className={`flex items-center gap-2 px-4 py-3 text-base font-medium border-b-2 transition-colors whitespace-nowrap max-md:px-3 max-md:text-sm ${
                  trainingTab === key ? 'border-indigo-600 text-indigo-600' : 'border-transparent text-slate-500 hover:text-slate-700'
                }`}>
                <Icon className="w-6 h-6 shrink-0 max-md:w-5 max-md:h-5" />{label}
              </button>
            ))}
          </div>

          {/* Tab 内容 */}
          <div className="flex-1 overflow-auto p-6 max-md:p-3">

            {/* Tab 1: 开始训练 */}
            {trainingTab === 'train' && (
              <div className="max-w-2xl mx-auto relative">
                <h3 className="text-xl font-semibold text-slate-800 mb-4 flex items-center gap-2 max-md:text-lg"><Settings className="w-6 h-6 text-indigo-500 shrink-0" />配置训练任务</h3>

                {/* 模型版本 + 模型选择 */}
                <div className="mb-6 grid grid-cols-2 gap-4 max-md:grid-cols-1 max-md:gap-3">
                  <div>
                    <label className="block text-base font-medium text-slate-700 mb-1">模型版本号</label>
                    <input type="text" value={trainingModelVersion} onChange={e => setTrainingModelVersion(e.target.value)}
                      className="w-full px-3 py-2.5 border border-slate-300 rounded-lg text-base focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500"
                      placeholder="例如 v1" />
                  </div>
                  <div>
                    <label className="block text-base font-medium text-slate-700 mb-1">模型选择</label>
                    <select value={trainingModelType} onChange={e => setTrainingModelType(e.target.value as 'catboost' | 'pinn')}
                      className="w-full px-3 py-2.5 border border-slate-300 rounded-lg text-base focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500">
                      <option value="catboost">CatBoost</option>
                      <option value="pinn">PINN（物理引导）</option>
                    </select>
                  </div>
                </div>

                {/* 训练数据条数 */}
                <div className="mb-6">
                  <label className="block text-base font-medium text-slate-700 mb-1">训练数据条数</label>
                  <input type="number" value={maxRows} onChange={e => setMaxRows(e.target.value)}
                    className="w-full px-3 py-2.5 border border-slate-300 rounded-lg text-base focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500"
                    placeholder="留空使用全部数据" min="1" />
                </div>

                {/* 开始训练 / 停止训练按钮 */}
                <div className="flex gap-3">
                  <button onClick={handleStartTraining} disabled={trainingStarting || trainingRunStatus === 'running' || !trainingModelVersion.trim()}
                    className="flex-1 py-2.5 bg-indigo-600 text-white text-base font-medium rounded-lg hover:bg-indigo-700 disabled:opacity-50 disabled:cursor-not-allowed flex items-center justify-center gap-2 transition-colors">
                    {(trainingStarting || trainingRunStatus === 'running') ? <Loader2 className="w-5 h-5 animate-spin" /> : <Play className="w-5 h-5" />}
                    {trainingStarting ? '启动中...' : trainingRunStatus === 'running' ? '训练中...' : '开始训练'}
                  </button>
                  {trainingRunStatus === 'running' && (
                    <button onClick={handleCancelTraining} disabled={cancelling}
                      className="flex-1 py-2.5 bg-red-500 text-white text-base font-medium rounded-lg hover:bg-red-600 disabled:opacity-50 disabled:cursor-not-allowed flex items-center justify-center gap-2 transition-colors">
                      {cancelling ? <Loader2 className="w-5 h-5 animate-spin" /> : <Square className="w-5 h-5" />}
                      {cancelling ? '正在停止...' : '停止训练'}
                    </button>
                  )}
                </div>

                {/* 训练日志区域 */}
                {trainingRunStatus !== 'idle' && (
                  <div className="mt-6 relative">
                    {/* 日志标题栏 */}
                    <div className="flex items-center justify-between mb-2">
                      <h4 className="text-xl font-semibold text-slate-700 flex items-center gap-2">
                        训练日志
                      </h4>
                      <span className={`inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-sm font-semibold ${
                        trainingRunStatus === 'running' ? 'bg-amber-100 text-amber-700' :
                        trainingRunStatus === 'completed' ? 'bg-green-100 text-green-700' :
                        trainingRunStatus === 'failed' ? 'bg-red-100 text-red-700' :
                        trainingRunStatus === 'cancelled' ? 'bg-orange-100 text-orange-700' : 'bg-slate-100 text-slate-600'
                      }`}>
                        {trainingRunStatus === 'running' && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
                        {trainingRunStatus === 'completed' && <CheckCircle className="w-3.5 h-3.5" />}
                        {trainingRunStatus === 'failed' && <X className="w-3.5 h-3.5" />}
                        {trainingRunStatus === 'cancelled' && <Square className="w-3.5 h-3.5" />}
                        {trainingRunStatus === 'running' ? '训练中' : trainingRunStatus === 'completed' ? '已完成' : trainingRunStatus === 'failed' ? `已失败${trainingStatus?.error ? `：${trainingStatus.error}` : ''}` : trainingRunStatus === 'cancelled' ? '已取消' : trainingRunStatus}
                      </span>
                    </div>

                    {/* 日志内容 */}
                    <div className="bg-gray-900 rounded-lg overflow-hidden relative">
                      <div className="h-[400px] overflow-y-auto p-4 font-mono text-base text-green-400 leading-relaxed">
                        {trainingLogs.length === 0 ? (
                          <div className="text-gray-500 italic">等待日志输出...</div>
                        ) : (
                          trainingLogs.map((line, idx) => (
                            <div key={idx} className="whitespace-pre-wrap break-all">{line}</div>
                          ))
                        )}
                        <div ref={trainingLogEndRef} />
                      </div>

                      {/* 训练指标：与日志黑框垂直居中 */}
                      {trainingStatus?.status === 'completed' && trainingStatus?.metrics && (
                        <div className="absolute bg-slate-50 rounded-lg p-4 border border-slate-200" style={{left: 'calc(100% + 1rem)', top: '50%', transform: 'translateY(-50%)'}}>
                          <h5 className="text-base font-semibold text-slate-700 mb-3 whitespace-nowrap">训练指标</h5>
                          <table className="text-base">
                            <thead>
                              <tr className="border-b border-slate-200">
                                <th className="py-1.5 text-left text-slate-500 font-medium pr-4">目标</th>
                                <th className="py-1.5 text-right text-slate-500 font-medium px-3">R²</th>
                                <th className="py-1.5 text-right text-slate-500 font-medium px-3">RMSE</th>
                                <th className="py-1.5 text-right text-slate-500 font-medium pl-3">MAE</th>
                              </tr>
                            </thead>
                            <tbody>
                              {Object.entries(trainingStatus.metrics).map(([k, v]: [string, any]) => (
                                <tr key={k} className="border-b border-slate-100 last:border-0">
                                  <td className="py-1.5 text-slate-600 font-medium pr-4 whitespace-nowrap">{k === 'ys' ? '屈服强度' : k === 'ts' ? '抗拉强度' : k === 'el' ? '延伸率' : k}</td>
                                  <td className="py-1.5 text-slate-800 text-right font-mono px-3">{v?.r2?.toFixed(4) ?? '-'}</td>
                                  <td className="py-1.5 text-slate-800 text-right font-mono px-3">{v?.rmse?.toFixed(4) ?? '-'}</td>
                                  <td className="py-1.5 text-slate-800 text-right font-mono pl-3">{v?.mae?.toFixed(4) ?? '-'}</td>
                                </tr>
                              ))}
                            </tbody>
                          </table>
                        </div>
                      )}
                    </div>


                  </div>
                )}
              </div>
            )}

            {/* Tab 2: 模型管理 */}
            {trainingTab === 'models' && (
              <div>
                <div className="flex items-center justify-between mb-4">
                  <h3 className="text-xl font-semibold text-slate-800">模型列表</h3>
                  <button onClick={fetchTrainingModels} className="flex items-center gap-1.5 text-base text-slate-500 hover:text-indigo-600 transition-colors">
                    <RefreshCw className={`w-5 h-5 ${trainingModelsLoading ? 'animate-spin' : ''}`} />刷新
                  </button>
                </div>
                {trainingModelsLoading ? (
                  <div className="flex items-center justify-center py-12 text-slate-400 text-base"><Loader2 className="w-5 h-5 animate-spin mr-2" />加载中...</div>
                ) : trainingModels.length === 0 ? (
                  <div className="text-center py-12 text-slate-400 text-base">暂无模型</div>
                ) : (
                  <div className="max-md:-mx-1 max-md:overflow-x-auto">
                  <table className="w-full text-base max-md:min-w-[560px]" style={{tableLayout: 'fixed'}}>
                    <colgroup>
                      <col style={{width: '24%'}} />
                      <col style={{width: '14%'}} />
                      <col style={{width: '12%'}} />
                      <col style={{width: '18%'}} />
                      <col style={{width: '32%'}} />
                    </colgroup>
                    <thead><tr className="text-left text-slate-500 border-b border-slate-200">
                      <th className="pb-2 pr-4">版本号</th>
                      <th className="pb-2 pr-4">模型</th>
                      <th className="pb-2 pr-4">数据条数</th>
                      <th className="pb-2 pr-4">状态</th>
                      <th className="pb-2">操作</th>
                    </tr></thead>
                    <tbody>
                      {trainingModels.map((m: any, idx: number) => (
                        <React.Fragment key={m.version || idx}>
                        <tr className={`border-b border-slate-100 transition-colors hover:bg-indigo-50 ${
                          m.is_active ? 'bg-green-50' : idx % 2 === 0 ? 'bg-white' : 'bg-slate-50'
                        }`}>
                          <td className="py-2.5 pr-4 font-medium text-slate-800">
                            {(m.version || '').replace(/^u\d+_/, '')}
                            {m.is_active && <span className="ml-2 inline-flex items-center gap-1 px-1.5 py-0.5 bg-green-100 text-green-700 text-sm rounded-full"><CheckCircle className="w-3.5 h-3.5" />当前使用</span>}
                          </td>
                          <td className="py-2.5 pr-4 text-slate-600">
                            {m.model_type === 'pinn' ? 'PINN' : m.model_type === 'catboost' ? 'CatBoost' : (m.model_type || '—')}
                          </td>
                          <td className="py-2.5 pr-4 text-slate-600">
                            {m.sample_count != null ? m.sample_count : '-'}
                          </td>
                          <td className="py-2.5 pr-4">
                            <span className={`inline-block px-2 py-0.5 rounded-full text-sm font-medium ${
                              m.is_active ? 'bg-green-100 text-green-700' : 'bg-slate-100 text-slate-600'
                            }`}>{m.is_active ? '已激活' : '未激活'}</span>
                          </td>
                          <td className="py-2.5">
                            <div className="flex items-center gap-2 w-full">
                            {!m.is_active && (
                              <button onClick={() => handleActivateModel(m.version)}
                                className="px-3 py-1 text-sm bg-indigo-600 text-white rounded-lg hover:bg-indigo-700 transition-colors">
                                激活
                              </button>
                            )}
                            <button onClick={() => handleViewModelLogs(m.version)}
                              className={`flex items-center gap-1 px-3 py-1 text-sm rounded-lg transition-colors ${
                                expandedModelLogs.has(m.version) ? 'bg-slate-700 text-white' : 'bg-slate-100 text-slate-600 hover:bg-slate-200'
                              }`}
                              disabled={modelLogLoading !== null && modelLogLoading !== m.version}>
                              {modelLogLoading === m.version ? (
                                <Loader2 className="w-3.5 h-3.5 animate-spin" />
                              ) : (
                                <FileText className="w-3.5 h-3.5" />
                              )}
                              {expandedModelLogs.has(m.version) ? '收起日志' : '查看日志'}
                            </button>
                            {!m.is_active && (
                              <button onClick={() => handleDeleteModel(m.version)}
                                className="ml-auto flex items-center gap-1 px-3 py-1 text-sm rounded-lg transition-colors bg-red-50 text-red-600 hover:bg-red-100"
                                disabled={deletingModel === m.version}>
                                {deletingModel === m.version ? (
                                  <Loader2 className="w-3.5 h-3.5 animate-spin" />
                                ) : (
                                  <Trash2 className="w-3.5 h-3.5" />
                                )}
                                删除
                              </button>
                            )}
                            </div>
                          </td>
                        </tr>
                        {expandedModelLogs.has(m.version) && modelLogDataMap[m.version] && (
                          <tr>
                            <td colSpan={5} className="p-0">
                              <div className="p-4 bg-slate-50 border-b border-slate-200">
                                <div className="flex gap-4">
                                  {/* 训练日志 - 终端风格 */}
                                  <div className="flex-1">
                                    <h5 className="text-base font-semibold text-slate-700 mb-2">训练日志</h5>
                                    <div className="bg-gray-900 rounded-lg p-4 font-mono text-base text-green-400 overflow-y-auto" style={{ height: '300px' }}>
                                      {modelLogDataMap[m.version].logs && modelLogDataMap[m.version].logs.length > 0 ? (
                                        modelLogDataMap[m.version].logs.map((log: string, i: number) => (
                                          <div key={i} className="leading-relaxed">{log}</div>
                                        ))
                                      ) : (
                                        <div className="text-slate-500">暂无日志</div>
                                      )}
                                    </div>
                                  </div>
                                  {/* 训练指标 */}
                                  {modelLogDataMap[m.version].metrics && Object.keys(modelLogDataMap[m.version].metrics).length > 0 && (
                                    <div className="shrink-0">
                                      <h5 className="text-base font-semibold text-slate-700 mb-2">训练指标</h5>
                                      <table className="text-base">
                                        <thead>
                                          <tr className="border-b border-slate-200">
                                            <th className="py-1.5 text-left text-slate-500 font-medium pr-4">目标</th>
                                            <th className="py-1.5 text-right text-slate-500 font-medium px-3">R²</th>
                                            <th className="py-1.5 text-right text-slate-500 font-medium px-3">RMSE</th>
                                            <th className="py-1.5 text-right text-slate-500 font-medium pl-3">MAE</th>
                                          </tr>
                                        </thead>
                                        <tbody>
                                          {Object.entries(modelLogDataMap[m.version].metrics).map(([k, v]: [string, any]) => (
                                            <tr key={k} className="border-b border-slate-100 last:border-0">
                                              <td className="py-1.5 text-slate-600 font-medium pr-4 whitespace-nowrap">{k === 'ys' ? '屈服强度' : k === 'ts' ? '抗拉强度' : k === 'el' ? '延伸率' : k}</td>
                                              <td className="py-1.5 text-slate-800 text-right font-mono px-3">{v?.r2?.toFixed(4) ?? '-'}</td>
                                              <td className="py-1.5 text-slate-800 text-right font-mono px-3">{v?.rmse?.toFixed(4) ?? '-'}</td>
                                              <td className="py-1.5 text-slate-800 text-right font-mono pl-3">{v?.mae?.toFixed(4) ?? '-'}</td>
                                            </tr>
                                          ))}
                                        </tbody>
                                      </table>
                                      {modelLogDataMap[m.version].sample_count && (
                                        <div className="mt-2 text-sm text-slate-500 leading-relaxed">样本数: {modelLogDataMap[m.version].sample_count}</div>
                                      )}
                                      {modelLogDataMap[m.version].created_at && (
                                        <div className="text-sm text-slate-500 leading-relaxed">训练时间: {modelLogDataMap[m.version].created_at}</div>
                                      )}
                                      {modelLogDataMap[m.version].duration && (
                                        <div className="text-sm text-slate-500 leading-relaxed">训练总耗时: {modelLogDataMap[m.version].duration}</div>
                                      )}
                                    </div>
                                  )}
                                </div>
                              </div>
                            </td>
                          </tr>
                        )}
                        </React.Fragment>
                      ))}
                    </tbody>
                  </table>
                  </div>
                )}
              </div>
            )}

          </div>
        </div>
      )}
    </>
  );
}
