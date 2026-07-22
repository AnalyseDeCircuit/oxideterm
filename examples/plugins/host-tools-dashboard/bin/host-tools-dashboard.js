#!/usr/bin/env node

// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

"use strict";

const readline = require("node:readline");

const PLUGIN_ID = "com.oxideterm.examples.host-tools-dashboard";
const TAB_ID = "dashboard";
const SIDEBAR_PANEL_ID = "dashboard-panel";
const ACTIVITY_ITEM_ID = "refresh-dashboard";
const MONITOR_ID = "system-facts";
const TAB_REGISTRATION_ID = "host-tools-dashboard-tab";
const SIDEBAR_REGISTRATION_ID = "host-tools-dashboard-sidebar";
const ACTIVITY_REGISTRATION_ID = "host-tools-dashboard-refresh-action";

let nextHostRequestId = 1;
let refreshInProgress = false;
let currentNodeId = null;
let currentOsType = "linux";
let currentRows = [];
let statusText = "Ready to sample an active node";
const pendingHostCalls = new Map();

// Stdout is exclusively reserved for versioned protocol frames.
function writeFrame(payload, requestId = null) {
  process.stdout.write(`${JSON.stringify({
    protocolVersion: 1,
    requestId,
    payload,
  })}\n`);
}

function respondOk(requestId, value) {
  writeFrame({
    requestId,
    result: {
      status: "ok",
      value,
    },
  }, requestId);
}

function respondError(requestId, code, message) {
  writeFrame({
    requestId,
    result: {
      status: "error",
      error: {
        code,
        message,
        recoverable: false,
      },
    },
  }, requestId);
}

function callHost(namespace, method, args = {}) {
  const requestId = `host-tools-dashboard-${nextHostRequestId++}`;
  writeFrame({
    type: "callHostApi",
    requestId,
    namespace,
    method,
    args,
  });
  return new Promise((resolve, reject) => {
    pendingHostCalls.set(requestId, { resolve, reject });
  });
}

function handleHostResponse(payload) {
  const pending = pendingHostCalls.get(payload.requestId);
  if (!pending || !payload.result) {
    return false;
  }
  pendingHostCalls.delete(payload.requestId);
  if (payload.result.status === "ok") {
    pending.resolve(payload.result.value);
  } else {
    const message = payload.result.error?.message || "Host API call failed";
    pending.reject(new Error(message));
  }
  return true;
}

function buildDashboardSchema(surfaceKind) {
  const contentControls = [
    {
      kind: "markdown",
      text: "**Host-rendered UI:** this view uses OxideTerm's shared components, theme, typography, focus, and interaction behavior.",
    },
    {
      kind: "statusBadge",
      label: statusText,
      tone: refreshInProgress ? "accent" : currentRows.length > 0 ? "success" : "neutral",
      strong: refreshInProgress,
    },
    {
      kind: "keyValue",
      label: "Monitor",
      value: MONITOR_ID,
    },
    {
      kind: "keyValue",
      label: "Target node",
      value: currentNodeId || "Automatic",
    },
    {
      kind: "keyValue",
      label: "Remote OS",
      value: currentOsType,
    },
    {
      kind: "select",
      id: "runtime-os",
      label: "Remote operating system",
      value: currentOsType,
      options: [
        { label: "Linux", value: "linux" },
        { label: "macOS", value: "macos" },
        { label: "BSD", value: "bsd" },
        { label: "Windows", value: "windows" },
      ],
    },
  ];

  if (currentRows.length > 0) {
    contentControls.push({
      kind: "table",
      id: "system-facts-table",
      label: "Sampled facts",
      columnDefs: [
        { key: "metric", label: "Metric", style: "primary" },
        { key: "value", label: "Value", style: "mono" },
      ],
      rows: currentRows,
    });
  } else {
    contentControls.push({
      kind: "emptyState",
      icon: "inbox",
      label: "No sample yet. Connect a node and select Refresh.",
    });
  }

  const actions = [{
    kind: "button",
    id: "refresh",
    label: refreshInProgress ? "Refreshing" : "Refresh",
    icon: "refresh-cw",
    variant: "default",
    loading: refreshInProgress,
    disabled: refreshInProgress,
  }];

  if (surfaceKind === "sidebarPanel") {
    actions.push({
      kind: "button",
      id: "open-dashboard-tab",
      label: "Open full dashboard",
      icon: "panel-left-open",
      variant: "outline",
    });
  }

  return {
    componentVersion: 1,
    kind: "form",
    title: "Host Tools Dashboard",
    description: "A reference plugin for custom Host Tools monitors.",
    sections: [
      {
        id: "overview",
        title: "Remote system facts",
        controls: [
          {
            kind: "card",
            variant: "inspector",
            gap: "normal",
            children: contentControls,
          },
          {
            kind: "toolbar",
            gap: "compact",
            children: actions,
          },
        ],
      },
    ],
  };
}

function registerSurface(kind, registrationId, metadata) {
  writeFrame({
    type: "registerContribution",
    registration: {
      registrationId,
      pluginId: PLUGIN_ID,
      kind,
      metadata,
    },
  });
}

function registerDashboardSurfaces() {
  registerSurface("tab", TAB_REGISTRATION_ID, {
    tabId: TAB_ID,
    schema: buildDashboardSchema("tab"),
  });
  registerSurface("sidebar-panel", SIDEBAR_REGISTRATION_ID, {
    panelId: SIDEBAR_PANEL_ID,
    schema: buildDashboardSchema("sidebarPanel"),
  });
}

function registerActivityAction() {
  registerSurface("activity-bar-item", ACTIVITY_REGISTRATION_ID, {
    itemId: ACTIVITY_ITEM_ID,
  });
}

function firstActiveNodeId(sessionSummary) {
  const nodes = Array.isArray(sessionSummary?.nodes) ? sessionSummary.nodes : [];
  const activeNode = nodes.find((node) =>
    node?.hasConnection && ["active", "connected"].includes(node?.state)
  );
  return typeof activeNode?.nodeId === "string" ? activeNode.nodeId : null;
}

function normalizedOsType(value) {
  return ["linux", "macos", "bsd", "windows"].includes(value) ? value : "linux";
}

async function refreshDashboard() {
  if (refreshInProgress) {
    return;
  }
  refreshInProgress = true;
  statusText = "Sampling";
  registerDashboardSurfaces();

  try {
    const configuredNodeId = await callHost("settings", "get", { key: "nodeId" });
    const configuredOsType = await callHost("settings", "get", { key: "osType" });
    const sessionSummary = await callHost("sessions", "getSummary");
    const extensions = await callHost("hostTools", "getExtensions");
    const monitorAvailable = Array.isArray(extensions)
      && extensions.some((extension) => extension?.id === MONITOR_ID);
    if (!monitorAvailable) {
      throw new Error("The system-facts monitor is not available");
    }

    const explicitNodeId = typeof configuredNodeId === "string"
      ? configuredNodeId.trim()
      : "";
    currentNodeId = explicitNodeId || firstActiveNodeId(sessionSummary);
    currentOsType = normalizedOsType(configuredOsType);
    if (!currentNodeId) {
      throw new Error("No active node is available; configure nodeId in plugin settings");
    }

    const sample = await callHost("hostTools", "runExtension", {
      nodeId: currentNodeId,
      osType: currentOsType,
      monitorId: MONITOR_ID,
    });
    if (!sample?.success) {
      throw new Error("The remote monitor command did not complete successfully");
    }
    currentRows = Array.isArray(sample.data) ? sample.data : [];
    statusText = sample.truncated
      ? `Ready — ${sample.rowCount || currentRows.length} rows, truncated`
      : `Ready — ${sample.rowCount || currentRows.length} rows`;
  } catch (error) {
    currentRows = [];
    // Host errors are already sanitized; never append command output or environment data.
    statusText = error instanceof Error ? error.message : "Refresh failed";
  } finally {
    refreshInProgress = false;
    registerDashboardSurfaces();
  }
}

async function handlePluginEvent(event) {
  if (event?.name !== "ui.event") {
    return { handled: false };
  }
  const controlId = event.payload?.controlId;
  if (event.payload?.type === "change" && controlId === "runtime-os") {
    currentOsType = normalizedOsType(event.payload.value);
    registerDashboardSurfaces();
    return { handled: true, action: "selectOs" };
  }
  if (controlId === "refresh") {
    await refreshDashboard();
    return { handled: true, action: "refresh" };
  }
  if (controlId === "open-dashboard-tab") {
    await callHost("ui", "openTab", { tabId: TAB_ID });
    return { handled: true, action: "openTab" };
  }
  return { handled: false };
}

async function handleRequest(envelope) {
  const payload = envelope?.payload;
  if (!payload) {
    return;
  }
  if (handleHostResponse(payload)) {
    return;
  }

  const requestId = payload.requestId;
  const requestType = payload.kind?.type;
  switch (requestType) {
    case "activate":
      registerDashboardSurfaces();
      registerActivityAction();
      writeFrame({ type: "runtimeReady" });
      respondOk(requestId, { activated: true });
      break;
    case "dispatchCommand":
      if (payload.kind.command !== "dashboard.refresh") {
        respondError(
          requestId,
          "unknown_command",
          `Unknown plugin command ${payload.kind.command}`,
        );
        break;
      }
      await refreshDashboard();
      respondOk(requestId, { refreshed: true });
      break;
    case "sendEvent": {
      const result = await handlePluginEvent(payload.kind.event);
      respondOk(requestId, result);
      break;
    }
    case "health":
      respondOk(requestId, { ok: true, refreshInProgress });
      break;
    case "deactivate":
    case "kill":
      respondOk(requestId, { stopped: true });
      process.exit(0);
      break;
    default:
      respondError(
        requestId,
        "unsupported_request",
        `Unsupported request ${requestType || "unknown"}`,
      );
  }
}

readline.createInterface({
  input: process.stdin,
  crlfDelay: Infinity,
}).on("line", (line) => {
  if (!line.trim()) {
    return;
  }
  let envelope;
  try {
    envelope = JSON.parse(line);
  } catch (_error) {
    process.stderr.write("Plugin received an invalid protocol frame.\n");
    return;
  }
  handleRequest(envelope).catch(() => {
    // Do not log protocol values because a future request may contain sensitive content.
    process.stderr.write("Plugin request handling failed.\n");
  });
});
