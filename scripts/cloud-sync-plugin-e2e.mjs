import assert from 'node:assert/strict';
import http from 'node:http';
import { createHash, webcrypto } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '..');

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
      useSyncExternalStore: (_subscribe, getSnapshot) => getSnapshot(),
    },
  },
};

const locale = JSON.parse(await readFile(path.join(repoRoot, 'plugins/official-cloud-sync/locales/en.json'), 'utf8'));

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

function sha256(bytes) {
  return `sha256:${createHash('sha256').update(Buffer.from(bytes)).digest('hex')}`;
}

function jsonBytes(value) {
  return new Uint8Array(Buffer.from(JSON.stringify(value), 'utf8'));
}

function parseBytes(bytes) {
  return JSON.parse(Buffer.from(bytes).toString('utf8'));
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitFor(label, predicate, timeoutMs = 5000) {
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

function createInitialSnapshot(name) {
  return {
    connections: [{ id: `${name}-conn-1`, name: `${name}-connection` }],
    forwards: [{ id: `${name}-forward-1` }],
  };
}

function toOxideMetadata(snapshot) {
  return {
    num_connections: snapshot.connections.length,
    description: 'fixture',
    has_embedded_keys: false,
  };
}

function toImportPreview(snapshot) {
  return {
    totalForwards: snapshot.forwards.length,
    willMerge: snapshot.connections.map((entry) => entry.id),
    willReplace: [],
    willSkip: [],
    willRename: [],
    hasEmbeddedKeys: false,
  };
}

function createDisposable(cleanup = () => {}) {
  return {
    dispose: cleanup,
  };
}

function createPluginContext({ backendType, endpoint, namespace }) {
  const commands = new Map();
  const toasts = [];
  const storage = new Map();
  const settings = new Map([
    ['backendType', backendType],
    ['authMode', 'none'],
    ['endpoint', endpoint],
    ['namespace', namespace],
    ['autoUploadEnabled', false],
    ['autoUploadIntervalMins', 60],
    ['defaultConflictStrategy', 'merge'],
  ]);
  const secrets = new Map([
    ['sync-password', 'pw'],
  ]);

  const savedConnectionsListeners = new Set();
  const savedForwardsListeners = new Set();
  let revisionCounter = 1;
  let localSnapshot = createInitialSnapshot(`${backendType}-local`);

  const buildLocalMetadata = () => ({
    savedConnectionsRevision: `conn-${revisionCounter}`,
    savedConnectionsUpdatedAt: new Date().toISOString(),
    savedForwardsRevision: `forward-${revisionCounter}`,
    settingsRevision: 'settings-1',
  });

  const emitLocalChange = () => {
    revisionCounter += 1;
    for (const listener of savedConnectionsListeners) {
      listener(localSnapshot.connections);
    }
    for (const listener of savedForwardsListeners) {
      listener(localSnapshot.forwards);
    }
  };

  const statusBarHandle = {
    update() {},
    dispose() {},
  };

  const context = {
    commands,
    toasts,
    setLocalSnapshot(snapshot) {
      localSnapshot = snapshot;
      emitLocalChange();
    },
    getLocalSnapshot() {
      return localSnapshot;
    },
    storage,
    ctx: {
      app: {
        getPlatform: () => 'macos',
        refreshAfterExternalSync: async () => {},
      },
      i18n: {
        t: resolveMessage,
        getLanguage: () => 'en',
        onLanguageChange: () => createDisposable(),
      },
      settings: {
        get: (key) => settings.get(key),
        set: (key, value) => settings.set(key, value),
        onChange: () => createDisposable(),
      },
      secrets: {
        get: async (key) => secrets.get(key) ?? null,
        set: async (key, value) => { secrets.set(key, value); },
        delete: async (key) => { secrets.delete(key); },
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
        registerStatusBarItem: () => statusBarHandle,
        registerCommand: (id, _opts, handler) => {
          commands.set(id, handler);
          return createDisposable(() => commands.delete(id));
        },
        showToast: (opts) => {
          toasts.push(opts);
        },
        showConfirm: async () => true,
        showProgress: () => ({
          report() {},
        }),
      },
      sync: {
        listSavedConnections: () => localSnapshot.connections,
        refreshSavedConnections: async () => localSnapshot.connections,
        onSavedConnectionsChange: (handler) => {
          savedConnectionsListeners.add(handler);
          return createDisposable(() => savedConnectionsListeners.delete(handler));
        },
        getLocalSyncMetadata: async () => buildLocalMetadata(),
        preflightExport: async () => ({ canExport: true, missing: [], warnings: [] }),
        exportOxide: async () => jsonBytes(localSnapshot),
        validateOxide: async (bytes) => toOxideMetadata(parseBytes(bytes)),
        previewImport: async (bytes) => toImportPreview(parseBytes(bytes)),
        importOxide: async (bytes) => {
          localSnapshot = parseBytes(bytes);
          revisionCounter += 1;
          return {
            imported: localSnapshot.connections.length,
            merged: 0,
            skipped: 0,
          };
        },
      },
      forward: {
        listSavedForwards: () => localSnapshot.forwards,
        onSavedForwardsChange: (handler) => {
          savedForwardsListeners.add(handler);
          return createDisposable(() => savedForwardsListeners.delete(handler));
        },
      },
    },
  };

  return context;
}

async function startHttpJsonServer() {
  const state = {
    exists: false,
    revision: null,
    etag: null,
    uploadedAt: null,
    deviceId: null,
    bytes: null,
    forceConflictOnce: false,
  };

  const server = http.createServer(async (req, res) => {
    const url = new URL(req.url, 'http://127.0.0.1');
    if (req.method === 'GET' && url.pathname.endsWith('/metadata')) {
      if (!state.exists) {
        res.writeHead(404, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: { code: 'remote_not_found' } }));
        return;
      }
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({
        exists: true,
        revision: state.revision,
        etag: state.etag,
        uploadedAt: state.uploadedAt,
        deviceId: state.deviceId,
        contentLength: state.bytes?.byteLength ?? 0,
      }));
      return;
    }

    if (req.method === 'PUT' && url.pathname.endsWith('/blob')) {
      const chunks = [];
      for await (const chunk of req) {
        chunks.push(chunk);
      }
      if (state.forceConflictOnce) {
        state.forceConflictOnce = false;
        res.writeHead(412, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          error: {
            code: 'etag_conflict_detected',
            message: 'Simulated ETag conflict',
            remoteEtag: state.etag,
            remoteRevision: state.revision,
          },
        }));
        return;
      }

      const bytes = new Uint8Array(Buffer.concat(chunks));
      state.bytes = bytes;
      state.exists = true;
      state.revision = req.headers['x-oxideterm-revision'];
      state.deviceId = req.headers['x-oxideterm-device-id'];
      state.etag = sha256(bytes);
      state.uploadedAt = new Date().toISOString();
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ ok: true, revision: state.revision, etag: state.etag }));
      return;
    }

    if (req.method === 'GET' && url.pathname.endsWith('/blob')) {
      if (!state.exists || !state.bytes) {
        res.writeHead(404);
        res.end();
        return;
      }
      res.writeHead(200, {
        'Content-Type': 'application/vnd.oxideterm.oxide',
        'Content-Length': String(state.bytes.byteLength),
        ETag: state.etag,
      });
      res.end(Buffer.from(state.bytes));
      return;
    }

    res.writeHead(404);
    res.end();
  });

  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const port = server.address().port;
  return {
    endpoint: `http://127.0.0.1:${port}`,
    state,
    close: () => new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve())),
  };
}

async function startWebDavServer() {
  const state = {
    namespaceReady: false,
    revision: null,
    etag: null,
    uploadedAt: null,
    deviceId: null,
    bytes: null,
    forceConflictOnce: false,
  };

  const server = http.createServer(async (req, res) => {
    const url = new URL(req.url, 'http://127.0.0.1');
    if (req.method === 'MKCOL') {
      if (state.namespaceReady) {
        res.writeHead(405);
      } else {
        state.namespaceReady = true;
        res.writeHead(201);
      }
      res.end();
      return;
    }

    if (req.method === 'PUT' && url.pathname.endsWith('/latest.oxide')) {
      const chunks = [];
      for await (const chunk of req) {
        chunks.push(chunk);
      }
      if (state.forceConflictOnce) {
        state.forceConflictOnce = false;
        res.writeHead(412, { ETag: state.etag ?? '' });
        res.end();
        return;
      }
      const bytes = new Uint8Array(Buffer.concat(chunks));
      state.bytes = bytes;
      state.etag = sha256(bytes);
      res.writeHead(200, { ETag: state.etag });
      res.end();
      return;
    }

    if (req.method === 'PUT' && url.pathname.endsWith('/latest.json')) {
      const chunks = [];
      for await (const chunk of req) {
        chunks.push(chunk);
      }
      const metadata = JSON.parse(Buffer.concat(chunks).toString('utf8'));
      state.namespaceReady = true;
      state.revision = metadata.revision;
      state.deviceId = metadata.deviceId;
      state.uploadedAt = metadata.uploadedAt;
      state.etag = metadata.etag;
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end('{}');
      return;
    }

    if (req.method === 'GET' && url.pathname.endsWith('/latest.json')) {
      if (!state.revision) {
        res.writeHead(404);
        res.end();
        return;
      }
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({
        revision: state.revision,
        etag: state.etag,
        uploadedAt: state.uploadedAt,
        deviceId: state.deviceId,
        contentLength: state.bytes?.byteLength ?? 0,
      }));
      return;
    }

    if (req.method === 'GET' && url.pathname.endsWith('/latest.oxide')) {
      if (!state.bytes) {
        res.writeHead(404);
        res.end();
        return;
      }
      res.writeHead(200, {
        'Content-Type': 'application/vnd.oxideterm.oxide',
        'Content-Length': String(state.bytes.byteLength),
        ETag: state.etag,
      });
      res.end(Buffer.from(state.bytes));
      return;
    }

    res.writeHead(404);
    res.end();
  });

  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const port = server.address().port;
  return {
    endpoint: `http://127.0.0.1:${port}/dav`,
    state,
    close: () => new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve())),
  };
}

async function runScenario(name, serverFactory) {
  const server = await serverFactory();
  const { activate, deactivate } = await import(pathToFileURL(path.join(repoRoot, 'plugins/official-cloud-sync/src/main.js')).href);
  const { getCloudSyncState } = await import(pathToFileURL(path.join(repoRoot, 'plugins/official-cloud-sync/src/store.js')).href);
  const harness = createPluginContext({
    backendType: name,
    endpoint: server.endpoint,
    namespace: 'team-a',
  });

  try {
    await activate(harness.ctx);

    harness.commands.get('cloud-sync-upload-now')();
    await waitFor(`${name} initial upload`, () => server.state.revision);
    assert.equal(getCloudSyncState().localDirty, false, `${name}: upload should clear local dirty state`);

    harness.setLocalSnapshot(createInitialSnapshot(`${name}-edited`));
    await waitFor(`${name} local dirty`, () => getCloudSyncState().localDirty === true);

    server.state.forceConflictOnce = true;
    harness.commands.get('cloud-sync-upload-now')();
    await waitFor(`${name} etag conflict toast`, () => harness.toasts.find((entry) => entry.title === 'Upload failed'));
    assert.match(getCloudSyncState().lastError, /remote snapshot changed during upload|远端快照已变化|retry/i, `${name}: should expose a conflict-oriented upload error`);

    const remoteSnapshot = {
      connections: [{ id: `${name}-remote-1`, name: `${name}-remote-connection` }],
      forwards: [{ id: `${name}-remote-forward-1` }],
    };
    server.state.bytes = jsonBytes(remoteSnapshot);
    server.state.etag = sha256(server.state.bytes);
    server.state.revision = `${name}-remote-rev-2`;
    server.state.deviceId = `${name}-remote-device`;
    server.state.uploadedAt = new Date().toISOString();

    harness.commands.get('cloud-sync-pull-preview')();
    await waitFor(`${name} pull import`, () => harness.getLocalSnapshot().connections[0]?.id === `${name}-remote-1`);
    assert.equal(getCloudSyncState().hasRollbackBackup, true, `${name}: pull should create a rollback backup`);

    harness.commands.get('cloud-sync-restore-backup')();
    await waitFor(`${name} restore backup`, () => harness.getLocalSnapshot().connections[0]?.id === `${name}-edited-conn-1`);
    assert.equal(getCloudSyncState().hasRollbackBackup, false, `${name}: restore should consume the rollback backup`);

    await deactivate();
    return {
      name,
      toasts: harness.toasts.map((entry) => entry.title),
      finalStatus: getCloudSyncState().status,
    };
  } finally {
    await server.close();
  }
}

const results = [];
results.push(await runScenario('webdav', startWebDavServer));
results.push(await runScenario('http-json', startHttpJsonServer));

console.log(JSON.stringify({ ok: true, results }, null, 2));
process.exit(0);