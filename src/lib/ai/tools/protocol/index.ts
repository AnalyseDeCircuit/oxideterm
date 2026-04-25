// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

export type {
  ToolCapability,
  ToolResultEnvelope,
  ToolResultError,
  ToolResultMeta,
  ToolRisk,
  ToolTarget,
  ToolTargetKind,
} from './types';
export {
  createToolResultEnvelope,
  fromLegacyToolResult,
  toLegacyToolResult,
} from './envelope';
export {
  inferToolRisk,
  isHighRiskToolRisk,
  sanitizeToolArguments,
} from './risk';
export type {
  ToolApprovalDecision,
  ToolApprovalPolicyInput,
} from './approvalPolicy';
export { decideToolApproval } from './approvalPolicy';
export { createToolTarget, hasTargetCapability } from './targets';
export type {
  TerminalBufferSnapshot,
  TerminalBufferSource,
  TerminalObserveData,
  TerminalObserveRequest,
  TerminalPromptDetection,
} from './terminalObserve';
export {
  detectTerminalPrompt,
  formatScreenSnapshot,
  getRenderedTextDelta,
  readBufferLineCount,
  readBufferRange,
  readBufferStats,
  readBufferTail,
  readRenderedBufferLines,
  readRenderedBufferTail,
  readRenderedBufferText,
  readTerminalScreen,
  renderedDeltaFromLineCount,
  renderedDeltaFromTextSnapshot,
  searchRenderedBuffer,
  terminalObserve,
} from './terminalObserve';
export type {
  TerminalOutputSubscription,
  TerminalWaitReason,
  TerminalWaitResult,
} from './terminalWait';
export {
  createTerminalOutputSubscription,
  waitForTerminalOutput,
} from './terminalWait';
export type {
  TerminalSendKind,
  TerminalSendRequest,
  TerminalSendResult,
} from './terminalSend';
export { terminalSend } from './terminalSend';
export type {
  TerminalRunData,
  TerminalRunRequest,
} from './terminalRun';
export { terminalRunRemote } from './terminalRun';
export type {
  FileDiffSummary,
  FileReadData,
  FileWriteData,
  FileWriteRequest,
} from './fileSafety';
export {
  buildFileDiffSummary,
  byteLengthOfText,
  hashTextContent,
  parseFileWriteRequest,
} from './fileSafety';
export type {
  TargetDiscoveryState,
  ToolCapabilityStatus,
} from './targetDiscovery';
export {
  buildCapabilityStatuses,
  buildToolTargets,
} from './targetDiscovery';
