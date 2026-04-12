import assert from 'node:assert/strict';
import { createHash, webcrypto } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const pluginRoot = process.env.OXIDE_CLOUD_SYNC_PLUGIN_ROOT
  ? path.resolve(process.env.OXIDE_CLOUD_SYNC_PLUGIN_ROOT)
  : path.join(process.env.HOME ?? '', '.oxideterm', 'plugins', 'com.oxideterm.cloud-sync');

if (!globalThis.crypto) {
  globalThis.crypto = webcrypto;
}
if (!globalThis.btoa) {
  globalThis.btoa = (value) => Buffer.from(value, 'binary').toString('base64');
}
if (!globalThis.atob) {
  globalThis.atob = (value) => Buffer.from(value, 'base64').toString('binary');
}

globalThis.window = {
  __OXIDE__: {
    React: {
      createElement: () => null,
      useState: (initial) => [initial, () => {}],
      useEffect: () => {},
      useRef: (value) => ({ current: value }),
      useSyncExternalStore: (_subscribe, getSnapshot) => getSnapshot(),
      useDeferredValue: (value) => value,
      startTransition: (callback) => callback(),
    },
    lucideReact: new Proxy({}, {
      get() {
        return () => null;
      },
    }),
  },
};

const APP_SECTION_IDS = [
  'general',
  'terminalAppearance',
  'terminalBehavior',
  'appearance',
  'connections',
  'fileAndEditor',
  'localTerminal',
];
const DEFAULT_SYNC_SCOPE = {
  syncConnections: true,
  syncForwards: true,
  syncAppSettings: true,
  appSettingsSections: [
    'general',
    'terminalAppearance',
    'terminalBehavior',
    'appearance',
    'connections',
    'fileAndEditor',
  ],
  includeLocalTerminalEnvVars: false,
  syncPluginSettings: true,
  pluginIds: ['plugin.alpha', 'plugin.beta'],
};
const OXIDE_CONTENT_TYPE = 'application/vnd.oxideterm.oxide';
const STRUCTURED_MANIFEST_FORMAT = 'structured-v1';
const STRUCTURED_MANIFEST_CONTENT_TYPE = 'application/vnd.oxideterm.cloud-sync.manifest+json';
const E2E_TIMEOUT_MS = Number(process.env.CLOUD_SYNC_E2E_TIMEOUT_MS ?? 5000);

const locale = JSON.parse(await readFile(path.join(pluginRoot, 'locales', 'en.json'), 'utf8'));

function resolveMessage(key, params = {}) {
  const segments = key.split('.');
  let current = locale;
  for (const segment of segments) {
    current = current?.[segment];
  }
  if (typeof current !== 'string') {
    return key;
  }
  return current.replace(/\{\{(\w+)\}\}/g, (_, name) => String(params[name] ?? ''));
}

function clone(value) {
  return JSON.parse(JSON.stringify(value));
}

function sha256(bytes) {
  return `sha256:${createHash('sha256').update(Buffer.from(bytes)).digest('hex')}`;
}

function jsonBytes(value) {
  return new Uint8Array(Buffer.from(JSON.stringify(value), 'utf8'));
}

function parseBytes(bytes) {
  return JSON.parse(Buffer.from(bytes).toString('utf8'));
}

function bytesToBase64(bytes) {
  return Buffer.from(bytes).toString('base64');
}

function base64ToBytes(value) {
  return new Uint8Array(Buffer.from(value ?? '', 'base64'));
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitFor(label, predicate, timeoutMs = E2E_TIMEOUT_MS) {
  const startedAt = Date.now();
  while (Date.now() - startedAt < timeoutMs) {
    const result = await predicate();
    if (result) {
      return result;
    }
    await delay(25);
  }
  throw new Error(`Timed out waiting for ${label}`);
}

function encodePathSegments(value) {
  return String(value ?? '')
    .split('/')
    .filter(Boolean)
    .map((segment) => encodeURIComponent(segment))
    .join('/');
}

function decodePathSegments(value) {
  return String(value ?? '')
    .split('/')
    .filter(Boolean)
    .map((segment) => decodeURIComponent(segment))
    .join('/');
}

function createDisposable(cleanup = () => {}) {
  return {
    dispose: cleanup,
  };
}

function createFixtureState(label) {
  const iso = new Date('2026-04-12T00:00:00.000Z').toISOString();
  return {
    connections: [
      {
        id: `${label}-conn-1`,
        name: `${label}-connection`,
        group: 'default',
        host: `${label}.example.com`,
        port: 22,
        username: 'oxide',
        auth_type: 'password',
        key_path: null,
        cert_path: null,
        created_at: iso,
        last_used_at: null,
        color: null,
        tags: [label],
        deleted: false,
      },
    ],
    forwards: [
      {
        id: `${label}-forward-1`,
        owner_connection_id: `${label}-conn-1`,
        forward_type: 'local',
        bind_address: '127.0.0.1',
        bind_port: 15432,
        target_host: 'db.internal',
        target_port: 5432,
        description: `${label} forward`,
        deleted: false,
      },
    ],
    appSettings: Object.fromEntries(
      APP_SECTION_IDS.map((sectionId) => [sectionId, {
        label: `${label}-${sectionId}`,
        enabled: true,
      }]),
    ),
    pluginSettings: {
      'plugin.alpha': { mode: `${label}-alpha`, enabled: true },
      'plugin.beta': { mode: `${label}-beta`, enabled: false },
    },
  };
}

function buildConnectionsSnapshot(data, revision) {
  return {
    revision,
    records: data.connections.map((connection) => ({
      id: connection.id,
      name: connection.name,
      deleted: Boolean(connection.deleted),
      payload: clone(connection),
    })),
  };
}

function buildForwardsSnapshot(data, revision) {
  return {
    revision,
    records: data.forwards.map((forward) => ({
      id: forward.id,
      deleted: Boolean(forward.deleted),
      payload: clone(forward),
    })),
  };
}

function buildOxidePayload(data, options = {}) {
  const selectedConnectionIds = Array.isArray(options.connectionIds)
    ? new Set(options.connectionIds)
    : null;
  const includeAllConnections = selectedConnectionIds == null;
  const selectedConnections = includeAllConnections
    ? data.connections.filter((connection) => !connection.deleted)
    : data.connections.filter((connection) => selectedConnectionIds.has(connection.id) && !connection.deleted);
  const selectedForwards = includeAllConnections
    ? data.forwards.filter((forward) => !forward.deleted)
    : [];
  const appSectionIds = options.includeAppSettings
    ? (options.selectedAppSettingsSections?.length ? options.selectedAppSettingsSections : Object.keys(data.appSettings))
    : [];
  const pluginIds = options.includePluginSettings
    ? (options.selectedPluginIds?.length ? options.selectedPluginIds : Object.keys(data.pluginSettings))
    : [];

  return {
    description: options.description ?? 'fixture',
    includeLocalTerminalEnvVars: Boolean(options.includeLocalTerminalEnvVars),
    connections: selectedConnections.map((entry) => clone(entry)),
    forwards: selectedForwards.map((entry) => clone(entry)),
    appSettings: Object.fromEntries(appSectionIds.map((sectionId) => [sectionId, clone(data.appSettings[sectionId] ?? {})])),
    pluginSettings: Object.fromEntries(pluginIds.map((pluginId) => [pluginId, clone(data.pluginSettings[pluginId] ?? {})])),
  };
}

function toOxideMetadata(payload) {
  return {
    num_connections: payload.connections.length,
    connection_names: payload.connections.map((entry) => entry.name),
    description: payload.description,
    has_embedded_keys: false,
    has_app_settings: Object.keys(payload.appSettings).length > 0,
    plugin_settings_count: Object.keys(payload.pluginSettings).length,
  };
}

function toPreview(payload) {
  return {
    totalConnections: payload.connections.length,
    totalForwards: payload.forwards.length,
    hasEmbeddedKeys: false,
    hasAppSettings: Object.keys(payload.appSettings).length > 0,
    appSettingsSections: Object.entries(payload.appSettings).map(([id, value]) => ({
      id,
      fieldKeys: Object.keys(value ?? {}),
      previewValues: clone(value ?? {}),
    })),
    pluginSettingsCount: Object.keys(payload.pluginSettings).length,
    pluginSettingsByPlugin: Object.fromEntries(Object.keys(payload.pluginSettings).map((pluginId) => [pluginId, 1])),
    forwardDetails: payload.forwards.map((forward) => ({
      ownerConnectionName: forward.owner_connection_id,
      direction: forward.forward_type,
      description: forward.description,
    })),
    records: payload.connections.map((connection) => ({
      id: connection.id,
      name: connection.name,
      action: 'import',
      reasonCode: 'new-connection',
      targetName: null,
      targetConnectionId: null,
    })),
  };
}

function buildStructuredDataset(data, label) {
  const connectionsRevision = `${label}-connections-r1`;
  const forwardsRevision = `${label}-forwards-r1`;
  const manifest = {
    format: STRUCTURED_MANIFEST_FORMAT,
    revision: `${label}-manifest-r1`,
    uploadedAt: new Date('2026-04-12T00:00:00.000Z').toISOString(),
    deviceId: `${label}-device`,
    contentType: STRUCTURED_MANIFEST_CONTENT_TYPE,
    scope: clone(DEFAULT_SYNC_SCOPE),
    sections: {
      connections: {
        revision: connectionsRevision,
        path: `structured/connections/${connectionsRevision}.json`,
        recordCount: data.connections.length,
        contentType: 'application/json',
      },
      forwards: {
        revision: forwardsRevision,
        path: `structured/forwards/${forwardsRevision}.json`,
        recordCount: data.forwards.length,
        contentType: 'application/json',
      },
      appSettings: {},
      pluginSettings: {},
    },
  };
  const objects = new Map();
  objects.set(manifest.sections.connections.path, {
    bytes: jsonBytes(buildConnectionsSnapshot(data, connectionsRevision)),
    contentType: 'application/json',
  });
  objects.set(manifest.sections.forwards.path, {
    bytes: jsonBytes(buildForwardsSnapshot(data, forwardsRevision)),
    contentType: 'application/json',
  });

  for (const sectionId of DEFAULT_SYNC_SCOPE.appSettingsSections) {
    const sectionRevision = `${label}-app-${sectionId}-r1`;
    const relativePath = `structured/settings/app/${sectionId}/${sectionRevision}.oxide`;
    manifest.sections.appSettings[sectionId] = {
      revision: sectionRevision,
      path: relativePath,
      contentType: OXIDE_CONTENT_TYPE,
    };
    objects.set(relativePath, {
      bytes: jsonBytes(buildOxidePayload(data, {
        connectionIds: [],
        includeAppSettings: true,
        selectedAppSettingsSections: [sectionId],
        includePluginSettings: false,
      })),
      contentType: OXIDE_CONTENT_TYPE,
    });
  }

  for (const pluginId of DEFAULT_SYNC_SCOPE.pluginIds) {
    const pluginRevision = `${label}-plugin-${pluginId}-r1`;
    const relativePath = `structured/settings/plugins/${pluginId}/${pluginRevision}.oxide`;
    manifest.sections.pluginSettings[pluginId] = {
      revision: pluginRevision,
      path: relativePath,
      contentType: OXIDE_CONTENT_TYPE,
    };
    objects.set(relativePath, {
      bytes: jsonBytes(buildOxidePayload(data, {
        connectionIds: [],
        includeAppSettings: false,
        includePluginSettings: true,
        selectedPluginIds: [pluginId],
      })),
      contentType: OXIDE_CONTENT_TYPE,
    });
  }

  manifest.sectionRevisions = {
    connections: manifest.sections.connections.revision,
    forwards: manifest.sections.forwards.revision,
    appSettings: Object.fromEntries(Object.entries(manifest.sections.appSettings).map(([sectionId, entry]) => [sectionId, entry.revision])),
    pluginSettings: Object.fromEntries(Object.entries(manifest.sections.pluginSettings).map(([pluginId, entry]) => [pluginId, entry.revision])),
  };

  return { manifest, objects };
}

function response(status, body = null, headers = {}) {
  return {
    status,
    headers,
    bodyBase64: body == null
      ? null
      : bytesToBase64(body instanceof Uint8Array ? body : jsonBytes(body)),
  };
}

function createBackendMock(kind, config) {
  const state = {
    kind,
    metadata: null,
    objects: new Map(),
    logs: [],
    collections: new Set(),
  };

  function storageKey(relativePath) {
    if (kind === 'dropbox') {
      return `/${[config.namespace, relativePath].filter(Boolean).join('/')}`;
    }
    if (kind === 'git' || kind === 's3') {
      return [config.namespace, relativePath].filter(Boolean).join('/');
    }
    return relativePath;
  }

  function setStructuredDataset(dataset) {
    state.metadata = clone(dataset.manifest);
    state.objects = new Map();
    for (const [relativePath, entry] of dataset.objects.entries()) {
      state.objects.set(storageKey(relativePath), {
        bytes: entry.bytes,
        contentType: entry.contentType,
        etag: sha256(entry.bytes),
        updatedAt: new Date().toISOString(),
      });
    }
    if (kind !== 'http-json') {
      const metadataBytes = jsonBytes(dataset.manifest);
      state.objects.set(storageKey('latest.json'), {
        bytes: metadataBytes,
        contentType: 'application/json',
        etag: sha256(metadataBytes),
        updatedAt: new Date().toISOString(),
      });
    }
  }

  function getObjectKeys() {
    return [...state.objects.keys()].sort();
  }

  function record(log) {
    state.logs.push(log);
  }

  function parseBodyBytes(request) {
    return request.bodyBase64 ? base64ToBytes(request.bodyBase64) : new Uint8Array();
  }

  async function handleHttpJson(request) {
    const url = new URL(request.url);
    const parts = url.pathname.split('/').filter(Boolean);
    const namespaceIndex = parts.indexOf('namespaces');
    const namespace = decodeURIComponent(parts[namespaceIndex + 1] ?? '');
    const resource = parts[namespaceIndex + 2] ?? '';
    const tail = parts.slice(namespaceIndex + 3).map((segment) => decodeURIComponent(segment)).join('/');
    record({ backend: kind, method: request.method, url: request.url, namespace, resource, logicalPath: tail || null });

    if (resource === 'metadata') {
      if (request.method === 'GET') {
        if (!state.metadata) {
          return response(404, { error: { code: 'remote_not_found' } }, { 'Content-Type': 'application/json' });
        }
        return response(200, state.metadata, { 'Content-Type': 'application/json', ETag: sha256(jsonBytes(state.metadata)) });
      }
      if (request.method === 'PUT') {
        state.metadata = parseBytes(parseBodyBytes(request));
        return response(200, { ok: true, revision: state.metadata.revision, etag: sha256(jsonBytes(state.metadata)) }, {
          'Content-Type': 'application/json',
          ETag: sha256(jsonBytes(state.metadata)),
        });
      }
    }

    if (resource === 'objects') {
      if (request.method === 'PUT') {
        const bytes = parseBodyBytes(request);
        state.objects.set(tail, {
          bytes,
          contentType: request.headers['Content-Type'] ?? request.headers['content-type'] ?? 'application/octet-stream',
          etag: sha256(bytes),
          updatedAt: new Date().toISOString(),
        });
        return response(200, null, { ETag: sha256(bytes) });
      }
      if (request.method === 'GET') {
        const entry = state.objects.get(tail);
        if (!entry) {
          return response(404);
        }
        return response(200, entry.bytes, {
          'Content-Type': entry.contentType,
          'Content-Length': String(entry.bytes.byteLength),
          ETag: entry.etag,
          'Last-Modified': entry.updatedAt,
        });
      }
    }

    return response(404);
  }

  async function handleWebDav(request) {
    const url = new URL(request.url);
    const basePath = new URL(config.endpoint).pathname.replace(/\/+$/, '');
    const namespaceRoot = `${basePath}/${encodePathSegments(config.namespace)}`.replace(/\/+$/, '');
    const relativePath = url.pathname.startsWith(`${namespaceRoot}/`)
      ? decodePathSegments(url.pathname.slice(namespaceRoot.length + 1))
      : '';
    const normalizedRequestPath = url.pathname.replace(/\/+$/, '');
    record({ backend: kind, method: request.method, url: request.url, logicalPath: relativePath || null });

    if (request.method === 'MKCOL') {
      state.collections.add(normalizedRequestPath);
      return response(201);
    }
    if (request.method === 'PROPFIND') {
      return response(state.collections.has(normalizedRequestPath) ? 207 : 404);
    }
    if (request.method === 'PUT') {
      const logicalParent = relativePath.split('/').slice(0, -1).join('/');
      const parentUrlPath = logicalParent
        ? `${namespaceRoot}/${encodePathSegments(logicalParent)}`.replace(/\/+$/, '')
        : namespaceRoot;
      if (!state.collections.has(parentUrlPath)) {
        return response(409);
      }

      const bytes = parseBodyBytes(request);
      state.objects.set(relativePath, {
        bytes,
        contentType: request.headers['Content-Type'] ?? request.headers['content-type'] ?? 'application/octet-stream',
        etag: sha256(bytes),
        updatedAt: new Date().toISOString(),
      });
      if (relativePath === 'latest.json') {
        state.metadata = parseBytes(bytes);
      }
      return response(200, null, { ETag: sha256(bytes) });
    }
    if (request.method === 'GET') {
      const entry = state.objects.get(relativePath);
      if (!entry) {
        return response(404);
      }
      return response(200, entry.bytes, {
        'Content-Type': entry.contentType,
        'Content-Length': String(entry.bytes.byteLength),
        ETag: entry.etag,
        'Last-Modified': entry.updatedAt,
      });
    }
    return response(404);
  }

  async function handleDropbox(request) {
    const url = new URL(request.url);
    const argHeader = request.headers['Dropbox-API-Arg'] ?? request.headers['dropbox-api-arg'] ?? null;
    const arg = argHeader ? JSON.parse(argHeader) : null;
    record({ backend: kind, method: request.method, url: request.url, logicalPath: arg?.path ?? null });

    if (url.pathname.endsWith('/files/create_folder_v2')) {
      return response(200, { metadata: { path_display: arg?.path ?? '/' } }, { 'Content-Type': 'application/json' });
    }
    if (url.pathname.endsWith('/files/upload')) {
      const bytes = parseBodyBytes(request);
      const etag = sha256(bytes);
      state.objects.set(arg.path, {
        bytes,
        contentType: request.headers['Content-Type'] ?? request.headers['content-type'] ?? 'application/octet-stream',
        etag,
        updatedAt: new Date().toISOString(),
      });
      if (arg.path.endsWith('/latest.json')) {
        state.metadata = parseBytes(bytes);
      }
      return response(200, { rev: etag, server_modified: new Date().toISOString() }, { 'Content-Type': 'application/json' });
    }
    if (url.pathname.endsWith('/files/download')) {
      const entry = state.objects.get(arg.path);
      if (!entry) {
        return response(409, { error_summary: 'path/not_found/' }, { 'Content-Type': 'application/json' });
      }
      return response(200, entry.bytes, {
        'Dropbox-API-Result': JSON.stringify({ rev: entry.etag, server_modified: entry.updatedAt }),
        'Content-Type': entry.contentType,
      });
    }
    return response(404);
  }

  async function handleGit(request) {
    const url = new URL(request.url);
    const match = url.pathname.match(/\/repos\/([^/]+)\/([^/]+)\/contents\/(.+)$/);
    const logicalPath = match ? decodePathSegments(match[3]) : null;
    record({ backend: kind, method: request.method, url: request.url, logicalPath });
    if (!logicalPath) {
      return response(404);
    }

    if (request.method === 'GET') {
      const entry = state.objects.get(logicalPath);
      if (!entry) {
        return response(404, { message: 'Not Found' }, { 'Content-Type': 'application/json' });
      }
      if ((request.headers.Accept ?? request.headers.accept ?? '').includes('application/vnd.github.raw+json')) {
        return response(200, entry.bytes, {
          'Content-Type': entry.contentType,
          ETag: entry.etag,
          'Last-Modified': entry.updatedAt,
        });
      }
      return response(200, {
        sha: entry.etag,
        size: entry.bytes.byteLength,
        encoding: 'base64',
        content: bytesToBase64(entry.bytes),
        path: logicalPath,
      }, { 'Content-Type': 'application/json' });
    }

    if (request.method === 'PUT') {
      const body = parseBytes(parseBodyBytes(request));
      const bytes = base64ToBytes(body.content);
      const existing = state.objects.get(logicalPath);
      if (body.sha && existing && body.sha !== existing.etag) {
        return response(409, { message: 'sha does not match' }, { 'Content-Type': 'application/json' });
      }
      const etag = sha256(bytes);
      state.objects.set(logicalPath, {
        bytes,
        contentType: logicalPath.endsWith('.json') ? 'application/json' : 'application/octet-stream',
        etag,
        updatedAt: new Date().toISOString(),
      });
      if (logicalPath.endsWith('latest.json')) {
        state.metadata = parseBytes(bytes);
      }
      return response(200, { content: { sha: etag, path: logicalPath } }, { 'Content-Type': 'application/json' });
    }

    return response(404);
  }

  async function handleS3(request) {
    const url = new URL(request.url);
    const endpointPath = new URL(config.endpoint).pathname.replace(/\/+$/, '');
    const relative = url.pathname.startsWith(endpointPath)
      ? url.pathname.slice(endpointPath.length)
      : url.pathname;
    const parts = relative.split('/').filter(Boolean);
    const bucket = decodeURIComponent(parts.shift() ?? '');
    const logicalPath = parts.map((segment) => decodeURIComponent(segment)).join('/');
    record({ backend: kind, method: request.method, url: request.url, bucket, logicalPath });

    if (bucket !== config.s3Bucket) {
      return response(404);
    }

    if (request.method === 'PUT') {
      const bytes = parseBodyBytes(request);
      const etag = sha256(bytes);
      state.objects.set(logicalPath, {
        bytes,
        contentType: request.headers['Content-Type'] ?? request.headers['content-type'] ?? 'application/octet-stream',
        etag,
        updatedAt: new Date().toISOString(),
      });
      if (logicalPath === `${config.namespace}/latest.json` || logicalPath === 'latest.json') {
        state.metadata = parseBytes(bytes);
      }
      return response(200, null, { ETag: etag });
    }

    if (request.method === 'GET') {
      const entry = state.objects.get(logicalPath);
      if (!entry) {
        return response(404);
      }
      return response(200, entry.bytes, {
        'Content-Type': entry.contentType,
        ETag: entry.etag,
        'Last-Modified': entry.updatedAt,
      });
    }

    return response(404);
  }

  async function invoke(request) {
    switch (kind) {
      case 'http-json':
        return handleHttpJson(request);
      case 'dropbox':
        return handleDropbox(request);
      case 'git':
        return handleGit(request);
      case 's3':
        return handleS3(request);
      case 'webdav':
      default:
        return handleWebDav(request);
    }
  }

  return {
    state,
    invoke,
    setStructuredDataset,
    getObjectKeys,
  };
}

function createPluginContext(config, remote) {
  const commands = new Map();
  const toasts = [];
  const storage = new Map([
    ['sync-scope', clone(DEFAULT_SYNC_SCOPE)],
  ]);
  const settings = new Map([
    ['backendType', config.backendType],
    ['authMode', config.authMode ?? 'none'],
    ['endpoint', config.endpoint ?? ''],
    ['namespace', config.namespace ?? 'default'],
    ['s3Bucket', config.s3Bucket ?? ''],
    ['s3Region', config.s3Region ?? ''],
    ['gitRepository', config.gitRepository ?? ''],
    ['gitBranch', config.gitBranch ?? 'main'],
    ['autoUploadEnabled', false],
    ['autoUploadIntervalMins', 60],
    ['defaultConflictStrategy', 'merge'],
  ]);
  const secrets = new Map([
    ['sync-password', 'pw'],
    ['backend-token', 'dropbox-token'],
    ['git-backend-token', 'git-token'],
    ['basic-username', 'oxide'],
    ['basic-password', 'secret'],
    ['s3-access-key-id', 'AKIAOXIDE'],
    ['s3-secret-access-key', 'oxide-secret'],
    ['s3-session-token', 'oxide-session'],
  ]);
  const settingsListeners = new Map();
  const savedConnectionsListeners = new Set();
  const savedForwardsListeners = new Set();
  let data = createFixtureState(`${config.backendType}-local`);
  let revisions = {
    connections: 1,
    forwards: 1,
    appSettings: Object.fromEntries(APP_SECTION_IDS.map((sectionId) => [sectionId, 1])),
    pluginSettings: Object.fromEntries(Object.keys(data.pluginSettings).map((pluginId) => [pluginId, 1])),
  };

  function emitSettingsChange(key) {
    for (const handler of settingsListeners.get(key) ?? []) {
      handler(settings.get(key));
    }
  }

  function emitConnectionsChange() {
    const snapshot = data.connections.filter((entry) => !entry.deleted).map((entry) => clone(entry));
    for (const handler of savedConnectionsListeners) {
      handler(snapshot);
    }
  }

  function emitForwardsChange() {
    const snapshot = data.forwards.filter((entry) => !entry.deleted).map((entry) => clone(entry));
    for (const handler of savedForwardsListeners) {
      handler(snapshot);
    }
  }

  function bumpAllRevisions() {
    revisions.connections += 1;
    revisions.forwards += 1;
    for (const sectionId of APP_SECTION_IDS) {
      revisions.appSettings[sectionId] = (revisions.appSettings[sectionId] ?? 0) + 1;
    }
    for (const pluginId of Object.keys(data.pluginSettings)) {
      revisions.pluginSettings[pluginId] = (revisions.pluginSettings[pluginId] ?? 0) + 1;
    }
  }

  function buildLocalMetadata() {
    return {
      savedConnectionsRevision: `connections-r${revisions.connections}`,
      savedConnectionsUpdatedAt: new Date().toISOString(),
      savedForwardsRevision: `forwards-r${revisions.forwards}`,
      settingsRevision: `settings-r${Math.max(...Object.values(revisions.appSettings))}`,
      appSettingsSectionRevisions: Object.fromEntries(APP_SECTION_IDS.map((sectionId) => [sectionId, `app-${sectionId}-r${revisions.appSettings[sectionId]}`])),
      pluginSettingsRevisions: Object.fromEntries(Object.keys(data.pluginSettings).map((pluginId) => [pluginId, `plugin-${pluginId}-r${revisions.pluginSettings[pluginId]}`])),
    };
  }

  function applyOxidePayload(payload, options = {}) {
    let imported = 0;
    if (payload.connections.length > 0) {
      data.connections = payload.connections.map((entry) => clone(entry));
      revisions.connections += 1;
      emitConnectionsChange();
      imported += payload.connections.length;
    }
    if (options.importForwards !== false && payload.forwards.length > 0) {
      data.forwards = payload.forwards.map((entry) => clone(entry));
      revisions.forwards += 1;
      emitForwardsChange();
    }
    if (options.importAppSettings !== false) {
      const selectedSections = options.selectedAppSettingsSections?.length
        ? options.selectedAppSettingsSections
        : Object.keys(payload.appSettings);
      for (const sectionId of selectedSections) {
        if (Object.prototype.hasOwnProperty.call(payload.appSettings, sectionId)) {
          data.appSettings[sectionId] = clone(payload.appSettings[sectionId]);
          revisions.appSettings[sectionId] = (revisions.appSettings[sectionId] ?? 0) + 1;
        }
      }
    }
    if (options.importPluginSettings !== false) {
      const selectedPluginIds = options.selectedPluginIds?.length
        ? options.selectedPluginIds
        : Object.keys(payload.pluginSettings);
      for (const pluginId of selectedPluginIds) {
        if (Object.prototype.hasOwnProperty.call(payload.pluginSettings, pluginId)) {
          data.pluginSettings[pluginId] = clone(payload.pluginSettings[pluginId]);
          revisions.pluginSettings[pluginId] = (revisions.pluginSettings[pluginId] ?? 0) + 1;
        }
      }
    }
    return {
      imported,
      merged: 0,
      skipped: 0,
    };
  }

  const context = {
    commands,
    toasts,
    getLocalData() {
      return clone(data);
    },
    setLocalData(nextData) {
      data = clone(nextData);
      bumpAllRevisions();
      emitConnectionsChange();
      emitForwardsChange();
    },
    ctx: {
      app: {
        getPlatform: () => 'macos',
        refreshAfterExternalSync: async () => {},
      },
      api: {
        invoke: async (command, request) => {
          if (command !== 'plugin_http_request') {
            throw new Error(`Unsupported command: ${command}`);
          }
          return remote.invoke(request);
        },
      },
      i18n: {
        t: resolveMessage,
        getLanguage: () => 'en',
        onLanguageChange: () => createDisposable(),
      },
      settings: {
        get: (key) => settings.get(key),
        set: (key, value) => {
          settings.set(key, value);
          emitSettingsChange(key);
        },
        onChange: (key, handler) => {
          const bucket = settingsListeners.get(key) ?? new Set();
          bucket.add(handler);
          settingsListeners.set(key, bucket);
          return createDisposable(() => bucket.delete(handler));
        },
      },
      secrets: {
        get: async (key) => secrets.get(key) ?? null,
        getMany: async (keys) => Object.fromEntries(keys.map((key) => [key, secrets.get(key) ?? null])),
        set: async (key, value) => {
          secrets.set(key, value);
        },
        delete: async (key) => {
          secrets.delete(key);
        },
      },
      storage: {
        get: (key) => storage.get(key) ?? null,
        set: (key, value) => storage.set(key, value),
        remove: (key) => storage.delete(key),
      },
      ui: {
        registerTabView: () => createDisposable(),
        openTab: () => {},
        registerSidebarPanel: () => createDisposable(),
        registerStatusBarItem: () => ({ update() {}, dispose() {} }),
        registerCommand: (id, _opts, handler) => {
          commands.set(id, handler);
          return createDisposable(() => commands.delete(id));
        },
        showToast: (entry) => {
          toasts.push(entry);
        },
        showConfirm: async () => true,
        showProgress: () => ({ report() {} }),
      },
      sync: {
        listSavedConnections: () => data.connections.filter((entry) => !entry.deleted).map((entry) => clone(entry)),
        refreshSavedConnections: async () => data.connections.filter((entry) => !entry.deleted).map((entry) => clone(entry)),
        onSavedConnectionsChange: (handler) => {
          savedConnectionsListeners.add(handler);
          return createDisposable(() => savedConnectionsListeners.delete(handler));
        },
        exportSavedConnectionsSnapshot: async () => buildConnectionsSnapshot(data, `connections-r${revisions.connections}`),
        applySavedConnectionsSnapshot: async (snapshot) => {
          data.connections = snapshot.records.filter((record) => !record.deleted).map((record) => clone(record.payload));
          revisions.connections += 1;
          emitConnectionsChange();
          return { imported: data.connections.length, merged: 0, skipped: 0 };
        },
        getLocalSyncMetadata: async () => buildLocalMetadata(),
        preflightExport: async () => ({ canExport: true, missing: [], warnings: [] }),
        exportOxide: async (options) => jsonBytes(buildOxidePayload(data, options)),
        validateOxide: async (bytes) => toOxideMetadata(parseBytes(bytes)),
        previewImport: async (bytes) => toPreview(parseBytes(bytes)),
        importOxide: async (bytes, _password, options = {}) => applyOxidePayload(parseBytes(bytes), options),
      },
      forward: {
        listSavedForwards: () => data.forwards.filter((entry) => !entry.deleted).map((entry) => clone(entry)),
        onSavedForwardsChange: (handler) => {
          savedForwardsListeners.add(handler);
          return createDisposable(() => savedForwardsListeners.delete(handler));
        },
        exportSavedForwardsSnapshot: async () => buildForwardsSnapshot(data, `forwards-r${revisions.forwards}`),
        applySavedForwardsSnapshot: async (snapshot) => {
          data.forwards = snapshot.records.filter((record) => !record.deleted).map((record) => clone(record.payload));
          revisions.forwards += 1;
          emitForwardsChange();
          return { applied: data.forwards.length };
        },
      },
    },
  };

  return context;
}

function selectionFromPreview(preview) {
  if (preview.mode === 'structured') {
    const selectedAppSettingsSections = Object.keys(preview.appSettingsEntries ?? {});
    const selectedPluginIds = Object.keys(preview.pluginSettingsEntries ?? {});
    return {
      importConnections: Boolean(preview.connectionsSnapshot),
      importForwards: Boolean(preview.forwardsSnapshot),
      importAppSettings: selectedAppSettingsSections.length > 0,
      selectedAppSettingsSections,
      importPluginSettings: selectedPluginIds.length > 0,
      selectedPluginIds,
      createRollbackBackup: false,
      conflictStrategy: 'merge',
    };
  }

  const selectedAppSettingsSections = Array.isArray(preview.summary?.appSettingsSections)
    ? preview.summary.appSettingsSections.map((entry) => entry.id)
    : [];
  const selectedPluginIds = Object.keys(preview.summary?.pluginSettingsByPlugin ?? {});
  return {
    importConnections: (preview.summary?.connections ?? 0) > 0,
    importForwards: (preview.summary?.forwards ?? 0) > 0,
    importAppSettings: selectedAppSettingsSections.length > 0,
    selectedAppSettingsSections,
    importPluginSettings: selectedPluginIds.length > 0,
    selectedPluginIds,
    createRollbackBackup: false,
    conflictStrategy: 'replace',
  };
}

function assertStructuredRemote(remote, expectedLabel) {
  assert.equal(remote.state.metadata?.format, STRUCTURED_MANIFEST_FORMAT, `${expectedLabel}: manifest format should be structured-v1`);
  assert.ok(remote.state.metadata?.revision, `${expectedLabel}: manifest revision should exist`);
  assert.deepEqual(
    remote.state.metadata?.sectionRevisions,
    {
      connections: remote.state.metadata?.sections?.connections?.revision ?? null,
      forwards: remote.state.metadata?.sections?.forwards?.revision ?? null,
      appSettings: Object.fromEntries(Object.entries(remote.state.metadata?.sections?.appSettings ?? {}).map(([sectionId, entry]) => [sectionId, entry.revision])),
      pluginSettings: Object.fromEntries(Object.entries(remote.state.metadata?.sections?.pluginSettings ?? {}).map(([pluginId, entry]) => [pluginId, entry.revision])),
    },
    `${expectedLabel}: manifest sectionRevisions should match section entries`,
  );
  assert.deepEqual(remote.state.metadata?.scope, DEFAULT_SYNC_SCOPE, `${expectedLabel}: manifest scope should round-trip`);
  assert.ok(remote.getObjectKeys().some((key) => key.includes('structured/connections/')), `${expectedLabel}: connections object should be written`);
  assert.ok(remote.getObjectKeys().some((key) => key.includes('structured/forwards/')), `${expectedLabel}: forwards object should be written`);
  assert.ok(remote.getObjectKeys().some((key) => key.includes('structured/settings/app/')), `${expectedLabel}: app settings objects should be written`);
  assert.ok(remote.getObjectKeys().some((key) => key.includes('structured/settings/plugins/')), `${expectedLabel}: plugin settings objects should be written`);
}

function assertBackendEncoding(kind, remote, namespace, bucket = null) {
  const encodedNamespace = encodePathSegments(namespace);
  const connectionsPath = remote.state.metadata?.sections?.connections?.path;
  assert.ok(connectionsPath, `${kind}: metadata should expose connections object path`);
  const encodedConnectionsPath = encodePathSegments(connectionsPath);
  if (kind === 'git') {
    assert.ok(
      remote.state.logs.some((entry) => entry.url.includes(`/contents/${encodedNamespace}/${encodedConnectionsPath}`)),
      'git: encoded namespace and object path should be used in contents URL',
    );
  } else if (kind === 's3') {
    assert.ok(
      remote.state.logs.some((entry) => entry.url.includes(`/${encodeURIComponent(bucket)}/${encodedNamespace}/${encodedConnectionsPath}`)),
      's3: encoded bucket, namespace, and object key path should be used',
    );
  } else if (kind === 'dropbox') {
    assert.ok(
      remote.state.logs.some((entry) => entry.logicalPath === `/${namespace}/${connectionsPath}`),
      'dropbox: namespace and object path should be preserved in Dropbox path argument',
    );
  }
}

const controllerModule = await import(pathToFileURL(path.join(pluginRoot, 'src', 'controller.js')).href);
const storeModule = await import(pathToFileURL(path.join(pluginRoot, 'src', 'store.js')).href);

const {
  activateController,
  deactivateController,
  uploadNow,
  pullAndPreview,
  restoreRollbackBackup,
  applyPendingPreviewSelection,
} = controllerModule;
const {
  getCloudSyncState,
  setCloudSyncState,
} = storeModule;
const initialStoreState = clone(getCloudSyncState());

async function resetPluginState() {
  await deactivateController();
  setCloudSyncState(clone(initialStoreState));
}

async function runFullScenario(kind, config) {
  const remote = createBackendMock(kind, config);
  const harness = createPluginContext(config, remote);

  try {
    await activateController(harness.ctx);
    await uploadNow();
    assertStructuredRemote(remote, kind);
    assert.equal(getCloudSyncState().localDirty, false, `${kind}: upload should clear local dirty state`);

    const edited = createFixtureState(`${kind}-edited`);
    harness.setLocalData(edited);
    await waitFor(`${kind} dirty`, () => getCloudSyncState().localDirty === true);

    const remoteDataset = buildStructuredDataset(createFixtureState(`${kind}-remote`), `${kind}-remote`);
    remote.setStructuredDataset(remoteDataset);

    await assert.rejects(() => uploadNow(), /remote|changed/i, `${kind}: upload should be blocked by remote conflict`);
    assert.equal(getCloudSyncState().autoUploadBlockedByConflict, true, `${kind}: conflict flag should be set`);

    const preview = await pullAndPreview();
    assert.equal(preview.mode, 'structured', `${kind}: pull preview should use structured mode`);
    const applySelection = selectionFromPreview(preview);
    applySelection.createRollbackBackup = true;
    await applyPendingPreviewSelection(applySelection);
    assert.equal(harness.getLocalData().connections[0].id, `${kind}-remote-conn-1`, `${kind}: remote connections should be applied`);
    assert.equal(harness.getLocalData().forwards[0].id, `${kind}-remote-forward-1`, `${kind}: remote forwards should be applied`);
    assert.equal(getCloudSyncState().hasRollbackBackup, true, `${kind}: conflict recovery should create rollback backup`);

    await restoreRollbackBackup();
    const backupPreview = await waitFor(`${kind} backup preview`, () => getCloudSyncState().pendingImportPreview?.source === 'backup' ? getCloudSyncState().pendingImportPreview : null);
    await applyPendingPreviewSelection(selectionFromPreview(backupPreview));
    assert.equal(harness.getLocalData().connections[0].id, `${kind}-edited-conn-1`, `${kind}: rollback restore should restore edited local data`);
    assert.equal(harness.getLocalData().forwards[0].id, `${kind}-edited-forward-1`, `${kind}: rollback restore should restore edited forwards`);
    assert.equal(harness.getLocalData().appSettings.general.label, `${kind}-edited-general`, `${kind}: rollback restore should restore app settings`);
    assert.equal(harness.getLocalData().pluginSettings['plugin.alpha'].mode, `${kind}-edited-alpha`, `${kind}: rollback restore should restore plugin settings`);

    return {
      backend: kind,
      revision: remote.state.metadata?.revision ?? null,
      objectCount: remote.getObjectKeys().length,
      toasts: harness.toasts.map((entry) => entry.title),
    };
  } finally {
    await resetPluginState();
  }
}

async function runMinimalScenario(kind, config) {
  const remote = createBackendMock(kind, config);
  const harness = createPluginContext(config, remote);

  try {
    await activateController(harness.ctx);
    await uploadNow();
    assertStructuredRemote(remote, kind);
    assertBackendEncoding(kind, remote, config.namespace, config.s3Bucket ?? null);

    const remoteDataset = buildStructuredDataset(createFixtureState(`${kind}-remote`), `${kind}-remote`);
    remote.setStructuredDataset(remoteDataset);
    const preview = await pullAndPreview();
    assert.equal(preview.mode, 'structured', `${kind}: structured preview should round-trip remote metadata`);
    assert.equal(preview.remoteMetadata.revision, remoteDataset.manifest.revision, `${kind}: preview should read written metadata revision`);
    assert.deepEqual(preview.remoteMetadata.sectionRevisions, remoteDataset.manifest.sectionRevisions, `${kind}: preview should read section revisions`);

    await applyPendingPreviewSelection(selectionFromPreview(preview));
    assert.equal(harness.getLocalData().connections[0].id, `${kind}-remote-conn-1`, `${kind}: minimal apply should import remote state`);
    assert.equal(harness.getLocalData().forwards[0].id, `${kind}-remote-forward-1`, `${kind}: minimal apply should import remote forwards`);
    assert.equal(harness.getLocalData().appSettings.general.label, `${kind}-remote-general`, `${kind}: minimal apply should import remote app settings`);
    assert.equal(harness.getLocalData().pluginSettings['plugin.alpha'].mode, `${kind}-remote-alpha`, `${kind}: minimal apply should import remote plugin settings`);

    return {
      backend: kind,
      revision: remote.state.metadata?.revision ?? null,
      objectCount: remote.getObjectKeys().length,
      requestCount: remote.state.logs.length,
    };
  } finally {
    await resetPluginState();
  }
}

const results = [];

results.push(await runFullScenario('http-json', {
  backendType: 'http-json',
  authMode: 'none',
  endpoint: 'https://sync.mock.local',
  namespace: 'team-a',
}));

results.push(await runFullScenario('webdav', {
  backendType: 'webdav',
  authMode: 'none',
  endpoint: 'https://webdav.mock.local/dav',
  namespace: 'team-a',
}));

results.push(await runMinimalScenario('git', {
  backendType: 'git',
  endpoint: 'https://git.mock.local/api',
  namespace: 'team space/prod+1',
  gitRepository: 'acme/oxide-sync',
  gitBranch: 'main',
}));

results.push(await runMinimalScenario('s3', {
  backendType: 's3',
  endpoint: 'https://s3.mock.local/storage',
  namespace: 'team space/prod+1',
  s3Bucket: 'oxide-bucket',
  s3Region: 'us-east-1',
}));

results.push(await runMinimalScenario('dropbox', {
  backendType: 'dropbox',
  endpoint: '',
  namespace: 'team space/prod+1',
}));

console.log(JSON.stringify({ ok: true, results }, null, 2));