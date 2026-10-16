// ============================================================
// Voice Port - 让客户端（GPUI 桌面）把一句话读出来
// ============================================================
//
// 只有 Host 在 runtime preferences 里下发 voiceEnabled 时才注入；端口在场即代表
// Speak 工具可见。端口绑定在单个会话上，调用方不传 sessionId。

export type VoiceSpeakKind = "attention" | "done" | "planReady" | "info";

export type VoiceSpeakRejectReason = "disabled" | "unfocusedOnly" | "unavailable";

export interface VoiceSpeakRequest {
  message: string;
  kind: VoiceSpeakKind;
}

export type VoiceSpeakResult =
  | { accepted: true }
  | { accepted: false; reason: VoiceSpeakRejectReason };

export interface VoicePort {
  /** Resolves once the client accepted or rejected the speech, not when playback ends. */
  speak(request: VoiceSpeakRequest, options?: { signal?: AbortSignal }): Promise<VoiceSpeakResult>;
}
