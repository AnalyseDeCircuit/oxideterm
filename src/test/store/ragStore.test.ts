import { beforeEach, describe, expect, it, vi } from 'vitest';

const apiMocks = vi.hoisted(() => ({
  ragCreateCollection: vi.fn(),
  ragListCollections: vi.fn().mockResolvedValue([]),
  ragDeleteCollection: vi.fn().mockResolvedValue(undefined),
  ragGetCollectionStats: vi.fn().mockResolvedValue({
    docCount: 1,
    chunkCount: 1,
    embeddedChunkCount: 0,
    lastUpdated: 123,
  }),
  ragAddDocument: vi.fn(),
  ragRemoveDocument: vi.fn().mockResolvedValue(undefined),
  ragListDocuments: vi.fn().mockResolvedValue({ documents: [], total: 0 }),
  ragGetPendingEmbeddings: vi.fn().mockResolvedValue([]),
  ragStoreEmbeddings: vi.fn().mockResolvedValue(0),
  ragSearch: vi.fn().mockResolvedValue([]),
  ragReindexCollection: vi.fn().mockResolvedValue(0),
  ragCancelReindex: vi.fn().mockResolvedValue(undefined),
  ragGetDocumentContent: vi.fn(),
  ragUpdateDocument: vi.fn(),
  ragCreateBlankDocument: vi.fn(),
  ragOpenDocumentExternal: vi.fn(),
}));

const fsMocks = vi.hoisted(() => ({
  readTextFile: vi.fn(),
  remove: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('@/lib/api', () => apiMocks);

vi.mock('@tauri-apps/plugin-fs', () => ({
  readTextFile: fsMocks.readTextFile,
  remove: fsMocks.remove,
}));

async function loadRagStore() {
  const mod = await import('@/store/ragStore');
  return mod.useRagStore;
}

describe('ragStore', () => {
  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    apiMocks.ragListCollections.mockResolvedValue([]);
    apiMocks.ragGetCollectionStats.mockResolvedValue({
      docCount: 1,
      chunkCount: 1,
      embeddedChunkCount: 0,
      lastUpdated: 123,
    });
    apiMocks.ragRemoveDocument.mockResolvedValue(undefined);
    apiMocks.ragOpenDocumentExternal.mockResolvedValue('/tmp/doc.md');
    apiMocks.ragGetDocumentContent.mockResolvedValue('stored content');
    apiMocks.ragUpdateDocument.mockResolvedValue({
      id: 'doc-1',
      collectionId: 'col-1',
      title: 'Doc 1',
      sourcePath: null,
      format: 'plaintext',
      chunkCount: 1,
      indexedAt: 200,
      version: 8,
    });
    apiMocks.ragAddDocument.mockResolvedValue({
      id: 'doc-2',
      collectionId: 'col-1',
      title: 'Doc 2',
      sourcePath: null,
      format: 'plaintext',
      chunkCount: 1,
      indexedAt: 200,
      version: 0,
    });
    apiMocks.ragCreateBlankDocument.mockResolvedValue({
      id: 'doc-3',
      collectionId: 'col-1',
      title: 'Blank',
      sourcePath: null,
      format: 'markdown',
      chunkCount: 0,
      indexedAt: 300,
      version: 0,
    });
    fsMocks.readTextFile.mockResolvedValue('updated content');
    fsMocks.remove.mockResolvedValue(undefined);
  });

  it('uses the captured document version when syncing after local state changes', async () => {
    const useRagStore = await loadRagStore();
    useRagStore.setState({
      selectedCollectionId: 'col-1',
      documents: [{
        id: 'doc-1',
        collectionId: 'col-1',
        title: 'Doc 1',
        sourcePath: null,
        format: 'plaintext',
        chunkCount: 1,
        indexedAt: 100,
        version: 7,
      }],
      documentTotal: 1,
    });

    await useRagStore.getState().openDocumentExternal('doc-1');
    useRagStore.setState({ documents: [] });

    const result = await useRagStore.getState().syncExternalEdits();

    expect(apiMocks.ragUpdateDocument).toHaveBeenCalledWith('doc-1', 'updated content', 7);
    expect(result).toEqual({ updated: true, docId: 'doc-1' });
    expect(useRagStore.getState().editingDocId).toBeNull();
    expect(useRagStore.getState().editingDocVersion).toBeNull();
  });

  it('clears editing state and temp file when reading the external file fails', async () => {
    const useRagStore = await loadRagStore();
    useRagStore.setState({
      editingDocId: 'doc-1',
      editFilePath: '/tmp/missing.md',
      editingDocVersion: 4,
    });
    fsMocks.readTextFile.mockRejectedValueOnce(new Error('ENOENT'));

    await expect(useRagStore.getState().syncExternalEdits()).rejects.toThrow('ENOENT');

    expect(fsMocks.remove).toHaveBeenCalledWith('/tmp/missing.md');
    expect(useRagStore.getState().editingDocId).toBeNull();
    expect(useRagStore.getState().editFilePath).toBeNull();
    expect(useRagStore.getState().editingDocVersion).toBeNull();
  });

  it('clears editing state and surfaces a stable sentinel on version conflicts', async () => {
    const useRagStore = await loadRagStore();
    const mod = await import('@/store/ragStore');
    useRagStore.setState({
      selectedCollectionId: 'col-1',
      documents: [{
        id: 'doc-1',
        collectionId: 'col-1',
        title: 'Doc 1',
        sourcePath: null,
        format: 'plaintext',
        chunkCount: 1,
        indexedAt: 100,
        version: 7,
      }],
      editingDocId: 'doc-1',
      editFilePath: '/tmp/doc.md',
      editingDocVersion: 7,
    });
    apiMocks.ragGetDocumentContent.mockResolvedValueOnce('old content');
    apiMocks.ragUpdateDocument.mockRejectedValueOnce(new Error('Version conflict: expected 7, found 8'));

    await expect(useRagStore.getState().syncExternalEdits()).rejects.toThrow(mod.RAG_SYNC_VERSION_CONFLICT_ERROR);

    expect(useRagStore.getState().editingDocId).toBeNull();
    expect(useRagStore.getState().editFilePath).toBeNull();
    expect(useRagStore.getState().editingDocVersion).toBeNull();
    expect(fsMocks.remove).not.toHaveBeenCalledWith('/tmp/doc.md');
  });

  it('keeps documentTotal in sync for selected collection mutations', async () => {
    const useRagStore = await loadRagStore();
    useRagStore.setState({
      selectedCollectionId: 'col-1',
      documents: [{
        id: 'doc-1',
        collectionId: 'col-1',
        title: 'Doc 1',
        sourcePath: null,
        format: 'plaintext',
        chunkCount: 1,
        indexedAt: 100,
        version: 1,
      }],
      documentTotal: 1,
    });

    await useRagStore.getState().addDocument('col-1', 'Doc 2', 'body', 'plaintext');
    expect(useRagStore.getState().documentTotal).toBe(2);

    await useRagStore.getState().createBlankDocument('col-1', 'Blank', 'markdown');
    expect(useRagStore.getState().documentTotal).toBe(3);

    await useRagStore.getState().removeDocument('doc-1');
    expect(useRagStore.getState().documentTotal).toBe(2);

    await useRagStore.getState().deleteCollection('col-1');
    expect(useRagStore.getState().selectedCollectionId).toBeNull();
    expect(useRagStore.getState().documentTotal).toBe(0);
  });
});