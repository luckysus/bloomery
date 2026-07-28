import { useCallback, useEffect, useRef, useState } from "react";
import { API_BASE } from "../services/api";

/**
 * 对话框实时语音输入 Hook（讯飞语音听写 IAT）。
 *
 * 流程：getUserMedia 采麦克风 → AudioContext + ScriptProcessor 拿 Float32 →
 * 线性重采样到 16kHz/16bit 单声道 PCM → 切成 1280B 帧、每 40ms 发一帧到
 * 后端 WebSocket(/api/asr/stream) → 后端签名代理讯飞 → 收 partial/final 文本。
 *
 * 后端在识别结果里已做 wpgs 动态修正累积，每次回传的都是「当前完整文本」，
 * 因此前端只需把它覆盖式回填到输入框即可（onText 覆盖语义）。
 */

const FRAME_BYTES = 1280; // 讯飞建议每帧 1280B（40ms@16k16bit）
const SEND_INTERVAL_MS = 40;
const OUT_SAMPLE_RATE = 16000;

interface AsrStatus {
  enabled: boolean;
  provider?: string;
  scene?: string;
}

interface UseSpeechToTextOptions {
  /** 收到识别文本（覆盖式，含 partial 与 final）。 */
  onText: (text: string) => void;
  /** 一段识别最终结束时回调（可选）。 */
  onFinal?: (text: string) => void;
}

function resolveWsUrl(): string {
  const base = (API_BASE || "").trim();
  if (base) {
    // 桌面端可能是绝对 http(s) 地址：转成 ws(s)
    return base.replace(/^http/i, "ws").replace(/\/$/, "") + "/api/asr/stream";
  }
  // Web 端同源：由当前页面协议决定 ws/wss
  const proto = window.location.protocol === "https:" ? "wss" : "ws";
  return `${proto}://${window.location.host}/api/asr/stream`;
}

export function useSpeechToText({ onText, onFinal }: UseSpeechToTextOptions) {
  const [available, setAvailable] = useState<boolean | null>(null);
  const [recording, setRecording] = useState(false);
  const [error, setError] = useState<string>("");

  const wsRef = useRef<WebSocket | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const ctxRef = useRef<AudioContext | null>(null);
  const sourceRef = useRef<MediaStreamAudioSourceNode | null>(null);
  const processorRef = useRef<ScriptProcessorNode | null>(null);
  const muteRef = useRef<GainNode | null>(null);
  const queueRef = useRef<Uint8Array[]>([]);
  const timerRef = useRef<number | null>(null);
  // 回调放进 ref，避免 start/stop 依赖变化导致重建
  const onTextRef = useRef(onText);
  const onFinalRef = useRef(onFinal);
  onTextRef.current = onText;
  onFinalRef.current = onFinal;

  // 挂载时查询后端是否已配置讯飞密钥
  useEffect(() => {
    let alive = true;
    fetch(`${API_BASE}/api/asr/status`, { credentials: "include" })
      .then((r) => (r.ok ? r.json() : { enabled: false }))
      .then((s: AsrStatus) => {
        if (alive) setAvailable(!!s.enabled);
      })
      .catch(() => {
        if (alive) setAvailable(false);
      });
    return () => {
      alive = false;
    };
  }, []);

  const teardownAudio = useCallback(() => {
    if (timerRef.current !== null) {
      window.clearInterval(timerRef.current);
      timerRef.current = null;
    }
    try {
      processorRef.current?.disconnect();
    } catch {
      /* ignore */
    }
    try {
      muteRef.current?.disconnect();
    } catch {
      /* ignore */
    }
    try {
      sourceRef.current?.disconnect();
    } catch {
      /* ignore */
    }
    try {
      void ctxRef.current?.close();
    } catch {
      /* ignore */
    }
    streamRef.current?.getTracks().forEach((t) => t.stop());
    processorRef.current = null;
    muteRef.current = null;
    sourceRef.current = null;
    ctxRef.current = null;
    streamRef.current = null;
    queueRef.current = [];
  }, []);

  const stop = useCallback(() => {
    // 通知后端结束，等最终结果再由 onclose 收尾
    try {
      wsRef.current?.send(JSON.stringify({ type: "end" }));
    } catch {
      /* ignore */
    }
    teardownAudio();
    setRecording(false);
  }, [teardownAudio]);

  const start = useCallback(async () => {
    if (recording) return;
    setError("");
    try {
      const stream = await navigator.mediaDevices.getUserMedia({
        audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true },
      });
      streamRef.current = stream;

      const Ctor: typeof AudioContext =
        window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
      const ctx = new Ctor();
      ctxRef.current = ctx;
      const source = ctx.createMediaStreamSource(stream);
      sourceRef.current = source;
      const processor = ctx.createScriptProcessor(4096, 1, 1);
      processorRef.current = processor;
      // 静音增益：让 processor 参与图但不外放，避免回声
      const mute = ctx.createGain();
      mute.gain.value = 0;
      muteRef.current = mute;

      const inRate = ctx.sampleRate;

      const ws = new WebSocket(resolveWsUrl());
      ws.binaryType = "arraybuffer";
      wsRef.current = ws;

      ws.onmessage = (ev) => {
        try {
          const msg = JSON.parse(String(ev.data));
          if (msg.type === "partial") {
            onTextRef.current(msg.text || "");
          } else if (msg.type === "final") {
            onTextRef.current(msg.text || "");
            onFinalRef.current?.(msg.text || "");
          } else if (msg.type === "error") {
            setError(msg.message || "语音识别错误");
          }
        } catch {
          /* ignore malformed frame */
        }
      };
      ws.onerror = () => setError("语音连接失败");

      await new Promise<void>((resolve, reject) => {
        ws.onopen = () => resolve();
        window.setTimeout(() => reject(new Error("连接超时")), 6000);
      });

      // 每 40ms 发一帧，控制发送节奏（讯飞对帧间隔敏感）
      timerRef.current = window.setInterval(() => {
        if (ws.readyState !== WebSocket.OPEN) return;
        const frame = queueRef.current.shift();
        if (frame) ws.send(frame);
      }, SEND_INTERVAL_MS);

      processor.onaudioprocess = (e) => {
        const input = e.inputBuffer.getChannelData(0);
        const ratio = inRate / OUT_SAMPLE_RATE;
        const outLen = Math.floor(input.length / ratio);
        const view = new DataView(new ArrayBuffer(outLen * 2));
        for (let i = 0; i < outLen; i++) {
          let s = input[Math.floor(i * ratio)] || 0;
          s = Math.max(-1, Math.min(1, s));
          view.setInt16(i * 2, s < 0 ? s * 0x8000 : s * 0x7fff, true);
        }
        const bytes = new Uint8Array(view.buffer);
        for (let off = 0; off < bytes.length; off += FRAME_BYTES) {
          queueRef.current.push(bytes.slice(off, Math.min(off + FRAME_BYTES, bytes.length)));
        }
      };

      source.connect(processor);
      processor.connect(mute);
      mute.connect(ctx.destination);
      // getUserMedia / ws.onopen 的 await 之后已脱离用户手势上下文，
      // 新建的 AudioContext 在 Chrome 自动播放策略下可能停在 suspended，
      // 导致 onaudioprocess 不触发、采不到音频。这里强制恢复到 running。
      if (ctx.state === "suspended") {
        try {
          await ctx.resume();
        } catch {
          /* ignore */
        }
      }
      setRecording(true);
    } catch (err) {
      const name = (err as { name?: string })?.name;
      setError(name === "NotAllowedError" ? "麦克风权限被拒绝" : "无法启动麦克风");
      teardownAudio();
      setRecording(false);
    }
  }, [recording, teardownAudio]);

  const toggle = useCallback(() => {
    if (recording) stop();
    else void start();
  }, [recording, start, stop]);

  // 卸载时彻底清理
  useEffect(
    () => () => {
      teardownAudio();
      try {
        wsRef.current?.close();
      } catch {
        /* ignore */
      }
    },
    [teardownAudio],
  );

  return { available, recording, error, start, stop, toggle };
}
