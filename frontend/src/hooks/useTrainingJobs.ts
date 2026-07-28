import React, { useCallback, useEffect, useRef, useState } from "react";
import { API_BASE } from "../services/api";
import {
  getLatestTrainingJob,
  getTrainingModelLogs,
  getTrainingModels,
  getTrainingStatus,
} from "../services/training";
import { isTrainingTerminalStatus, normalizeTrainingRunStatus, type TrainingRunStatus } from "../utils/trainingStatus";

export function useTrainingJobs(showTraining: boolean) {
  const [trainingTab, setTrainingTab] = useState<'train' | 'models'>('train');
  const [trainingModelVersion, setTrainingModelVersion] = useState('v1');
  const [trainingModelType, setTrainingModelType] = useState<'catboost' | 'pinn'>('catboost');
  const [trainingJobId, setTrainingJobId] = useState<string | null>(null);
  const [trainingStatus, setTrainingStatus] = useState<any>(null);
  const [trainingStarting, setTrainingStarting] = useState(false);
  const trainingAbortRef = useRef<AbortController | null>(null);
  const [trainingModels, setTrainingModels] = useState<any[]>([]);
  const [trainingModelsLoading, setTrainingModelsLoading] = useState(false);
  const [expandedModelLogs, setExpandedModelLogs] = useState<Set<string>>(new Set());
  const [modelLogDataMap, setModelLogDataMap] = useState<Record<string, any>>({});
  const [modelLogLoading, setModelLogLoading] = useState<string | null>(null);
  const trainingPollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const trainingPollFailCountRef = useRef(0);
  const [trainingLogs, setTrainingLogs] = useState<string[]>([]);
  const [trainingRunStatus, setTrainingRunStatus] = useState<TrainingRunStatus>('idle');
  const [maxRows, setMaxRows] = useState('');
  const [cancelling, setCancelling] = useState(false);                          
  
  const trainingLogEndRef = useRef<HTMLDivElement | null>(null);
  const [deletingModel, setDeletingModel] = useState<string | null>(null);

  const fetchTrainingModels = useCallback(async () => {
    setTrainingModelsLoading(true);
    try {
      const d = await getTrainingModels();
      setTrainingModels(d.models || []);
    } catch { setTrainingModels([]); }
    finally { setTrainingModelsLoading(false); }
  }, []);

  const stopTrainingPoll = useCallback(() => {
    if (trainingPollRef.current) { clearInterval(trainingPollRef.current); trainingPollRef.current = null; }
  }, []);

  /** 清除训练持久化数据并重置状态 */
  const clearTrainingPersistence = useCallback(() => {
    setTrainingRunStatus('idle');
    setTrainingJobId(null);
    setTrainingStatus(null);
    setTrainingLogs([]);
  }, []);

  const startTrainingPoll = useCallback((jobId: string, isRestore = false) => {
    stopTrainingPoll();
    trainingPollFailCountRef.current = 0;
    const poll = async () => {
      try {
        const resp = await getTrainingStatus(jobId);
        if (!resp.ok) {
          // 后端返回 404 等错误，说明 jobId 已失效
          if (resp.status === 404 || isRestore) {
            stopTrainingPoll();
            clearTrainingPersistence();
            setTrainingLogs(prev => [...prev, '[系统] 后端服务已重启，训练任务不存在，状态已重置']);
            return;
          }
          // 非恢复场景下累计失败次数
          trainingPollFailCountRef.current += 1;
          if (trainingPollFailCountRef.current >= 3) {
            stopTrainingPoll();
            clearTrainingPersistence();
            setTrainingLogs(prev => [...prev, '[系统] 后端服务不可用，训练状态已重置']);
          }
          return;
        }
        // 请求成功，重置失败计数
        trainingPollFailCountRef.current = 0;
        isRestore = false; // 第一次成功后不再视为恢复
        const d = await resp.json();
        setTrainingStatus(d);
        if (Array.isArray(d.logs)) {
          setTrainingLogs(d.logs.filter((l: string) => l && l.trim()));
        }
        const normalizedStatus = normalizeTrainingRunStatus(d.status);
        if (isTrainingTerminalStatus(d.status)) {
          setTrainingRunStatus(normalizedStatus);
          stopTrainingPoll();
          setCancelling(false);
        }
      } catch {
        // 网络错误 / 服务不可达
        if (isRestore) {
          stopTrainingPoll();
          clearTrainingPersistence();
          setTrainingLogs(prev => [...prev, '[系统] 后端服务不可达，训练状态已重置']);
          return;
        }
        trainingPollFailCountRef.current += 1;
        if (trainingPollFailCountRef.current >= 3) {
          stopTrainingPoll();
          clearTrainingPersistence();
          setTrainingLogs(prev => [...prev, '[系统] 后端服务不可用，训练状态已重置']);
        }
      }
    };
    poll();
    trainingPollRef.current = setInterval(poll, 500); //日志轮询时间 0.5s
  }, [stopTrainingPoll, clearTrainingPersistence]);

  const waitForTrainingIdle = useCallback(async (jobId: string, timeoutMs = 60000) => {
    const startedAt = Date.now();
    while (Date.now() - startedAt < timeoutMs) {
      try {
        const resp = await getTrainingStatus(jobId);
        if (resp.status === 404) {
          return true;
        }
        if (resp.ok) {
          const d = await resp.json();
          setTrainingStatus(d);
          if (Array.isArray(d.logs)) {
            setTrainingLogs(d.logs.filter((l: string) => l && l.trim()));
          }
          if (isTrainingTerminalStatus(d.status)) {
            return true;
          }
        }
      } catch {
        return true;
      }
      await new Promise(resolve => setTimeout(resolve, 500));
    }
    return false;
  }, []);

  const restoreLatestTrainingJob = useCallback(async () => {
    try {
      const resp = await getLatestTrainingJob();
      if (!resp.ok) return;
      const latest = await resp.json();
      if (!latest?.job_id) return;
      const normalizedStatus = normalizeTrainingRunStatus(latest.status);
      if (normalizedStatus !== 'running') {
        clearTrainingPersistence();
        return;
      }
      setTrainingJobId(latest.job_id);
      setTrainingStatus(latest);
      if (Array.isArray(latest.logs)) {
        setTrainingLogs(latest.logs.filter((l: string) => l && l.trim()));
      }
      setTrainingRunStatus('running');
      startTrainingPoll(latest.job_id, true);
    } catch {
      // 恢复失败不影响手动重新训练
    }
  }, [startTrainingPoll, clearTrainingPersistence]);

  // ======== 页面可见性变化时恢复训练轮询 ========
  useEffect(() => {
    const handleVisibilityChange = () => {
      if (!document.hidden) {
        if (trainingJobId && trainingRunStatus === 'running') {
          startTrainingPoll(trainingJobId, true);
        }
      }
    };
    document.addEventListener('visibilitychange', handleVisibilityChange);
    return () => document.removeEventListener('visibilitychange', handleVisibilityChange);
  }, [trainingJobId, trainingRunStatus, startTrainingPoll]);

  const handleStartTraining = async () => {
    if (!trainingModelVersion.trim()) { alert('请输入模型版本号'); return; }
    setTrainingStarting(true);
    try {
      const existingJobId = trainingJobId;
      if (existingJobId) {
        setTrainingLogs(prev => [...prev, '[系统] 正在确认上一轮训练是否已结束...']);
        const isIdle = await waitForTrainingIdle(existingJobId);
        if (!isIdle) {
          const msg = '上一轮训练仍在停止中，请稍后再开始新的训练';
          alert(msg);
          setTrainingRunStatus('running');
          startTrainingPoll(existingJobId, true);
          setTrainingLogs(prev => [...prev, `[系统] ${msg}`]);
          return;
        }
        setTrainingJobId(null);
      }

      setTrainingStatus(null);
      setTrainingLogs([]);
      setTrainingRunStatus('running');
      const abortCtrl = new AbortController();
      trainingAbortRef.current = abortCtrl;
      const resp = await fetch(`${API_BASE}/api/training/start`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ model_version: trainingModelVersion, max_rows: maxRows ? parseInt(maxRows) : null, config: { model_type: trainingModelType } }),
        signal: abortCtrl.signal,
      });
      if (resp.status === 409) {
        const errData = await resp.json().catch(() => ({} as any));
        const conflictMsg = errData.detail || '启动训练失败';
        alert(conflictMsg);
        setTrainingRunStatus('failed');
        setTrainingLogs(prev => [...prev, `[系统] ${conflictMsg}`]);
        return;
      }
      // 后端重启/网关异常时 nginx 会返回 HTML 错误页，不能直接当 JSON 解析
      const d = await resp.json().catch(() => null);
      if (!resp.ok || !d) {
        const failMsg = (d && d.detail) || `服务暂时不可用（${resp.status}），请稍后重试`;
        alert('启动训练失败: ' + failMsg);
        setTrainingRunStatus('failed');
        setTrainingLogs(prev => [...prev, `[系统] 启动训练失败: ${failMsg}`]);
        return;
      }
      if (d.job_id) {
        setTrainingJobId(d.job_id);
        startTrainingPoll(d.job_id);
      }
    } catch (e: any) {
      if (e.name === 'AbortError') {
        setTrainingRunStatus('cancelled');
        setTrainingLogs(prev => [...prev, '[系统] 训练启动已取消']);
      } else {
        alert('启动训练失败: ' + (e.message || '未知错误'));
        setTrainingRunStatus('failed');
      }
    }
    finally { setTrainingStarting(false); trainingAbortRef.current = null; }
  };

  const handleCancelTraining = async () => {
    setCancelling(true);
    // 如果还在启动阶段（没有 job_id），直接 abort 请求
    if (trainingAbortRef.current) {
      trainingAbortRef.current.abort();
    }
    // 如果已有 job_id，发送取消请求到后端
    if (trainingJobId) {
      try {
        const resp = await fetch(`${API_BASE}/api/training/cancel/${trainingJobId}`, { method: 'POST' });
        if (resp.ok) {
          const data = await resp.json().catch(() => ({}));
          setTrainingStatus(data);
          if (Array.isArray(data.logs)) {
            setTrainingLogs(data.logs.filter((line: string) => line && line.trim()));
          }
          const normalizedStatus = normalizeTrainingRunStatus(data.status);
          setTrainingRunStatus(normalizedStatus === 'idle' ? 'cancelled' : normalizedStatus);
          if (isTrainingTerminalStatus(data.status) || normalizedStatus === 'idle') {
            stopTrainingPoll();
          }
        } else if (resp.status === 404) {
          setTrainingRunStatus('cancelled');
          stopTrainingPoll();
          setTrainingLogs(prev => [...prev, '[系统] 远程训练任务不存在，已清理本地状态']);
        }
      } catch (e: any) { console.error('停止训练失败:', e.message); }
    }
    setTimeout(() => setCancelling(false), 2000);
  };

  const handleActivateModel = async (version: string) => {
    try {
      await fetch(`${API_BASE}/api/training/models/${version}/activate`, { method: 'POST' });
      await fetchTrainingModels();
    } catch (e: any) { alert('激活失败: ' + (e.message || '未知错误')); }
  };

  const handleDeleteModel = async (version: string) => {
    if (!confirm(`确定要删除模型版本 ${version} 吗？此操作不可恢复！`)) return;
    setDeletingModel(version);
    try {
      const resp = await fetch(`${API_BASE}/api/training/models/${version}`, { method: 'DELETE' });
      if (!resp.ok) {
        const data = await resp.json().catch(() => ({}));
        throw new Error(data.detail || `删除失败 (${resp.status})`);
      }
      await fetchTrainingModels();
    } catch (e: any) { alert('删除失败: ' + (e.message || '未知错误')); }
    finally { setDeletingModel(null); }
  };

  const handleViewModelLogs = async (version: string) => {
    if (expandedModelLogs.has(version)) {
      setExpandedModelLogs(prev => { const next = new Set(prev); next.delete(version); return next; });
      return;
    }
    setModelLogLoading(version);
    try {
      const resp = await getTrainingModelLogs(version);
      if (resp.ok) {
        const data = await resp.json();
        setModelLogDataMap(prev => ({ ...prev, [version]: data }));
        setExpandedModelLogs(prev => new Set(prev).add(version));
      }
    } catch (e) { console.error(e); }
    finally { setModelLogLoading(null); }
  };

  useEffect(() => {
    if (showTraining) {
      void restoreLatestTrainingJob();
      if (trainingTab === 'models') fetchTrainingModels();
    } else { stopTrainingPoll(); }
  }, [showTraining, trainingTab, fetchTrainingModels, restoreLatestTrainingJob, stopTrainingPoll]);

  useEffect(() => { return () => { stopTrainingPoll(); }; }, [stopTrainingPoll]);

  // 训练日志自动滚动到底部
  useEffect(() => {
    trainingLogEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [trainingLogs]);

  return {
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
  };
}
