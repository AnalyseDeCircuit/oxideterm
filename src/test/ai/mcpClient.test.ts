import { beforeEach, describe, expect, it, vi } from 'vitest';

const apiMock = vi.hoisted(() => ({
  mcpSpawnServer: vi.fn(),
  mcpSendRequest: vi.fn(),
  mcpCloseServer: vi.fn(),
  getAiProviderApiKey: vi.fn(),
  setAiProviderApiKey: vi.fn(),
  deleteAiProviderApiKey: vi.fn(),
}));

vi.mock('@/lib/api', () => ({
  api: apiMock,
}));

describe('mcpClient', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.unstubAllGlobals();
    apiMock.mcpCloseServer.mockResolvedValue(undefined);
  });

  it('connects a stdio MCP server without calling tools/list when tools capability is absent', async () => {
    const { connectMcpServer } = await import('@/lib/ai/mcp/mcpClient');
    const state = {
      config: {
        id: 'srv-1',
        name: 'resources-only',
        transport: 'stdio' as const,
        command: 'npx',
        args: ['-y', '@example/server'],
        env: { API_TOKEN: 'secret' },
        enabled: true,
      },
      status: 'connecting' as const,
      tools: [],
      resources: [],
    };

    apiMock.mcpSpawnServer.mockResolvedValue('runtime-1');
    apiMock.mcpSendRequest.mockImplementation(async (_runtimeId: string, method: string) => {
      if (method === 'initialize') {
        return { capabilities: { resources: {} } };
      }
      if (method === 'notifications/initialized') {
        return null;
      }
      if (method === 'resources/list') {
        return { resources: [{ uri: 'file:///a', name: 'A' }] };
      }
      throw new Error(`unexpected method ${method}`);
    });

    const result = await connectMcpServer(state);

    expect(result.status).toBe('connected');
    expect(result.tools).toEqual([]);
    expect(result.resources).toEqual([{ uri: 'file:///a', name: 'A' }]);
    expect(result.capabilities).toEqual({ resources: {} });
    expect(apiMock.mcpSpawnServer).toHaveBeenCalledWith('npx', ['-y', '@example/server'], { API_TOKEN: 'secret' });
    expect(apiMock.mcpSendRequest.mock.calls.map((call) => call[1])).toEqual([
      'initialize',
      'notifications/initialized',
      'resources/list',
    ]);
  });

  it('refreshMcpTools is a no-op for connected servers without tools capability', async () => {
    const { refreshMcpTools } = await import('@/lib/ai/mcp/mcpClient');

    const tools = await refreshMcpTools({
      config: {
        id: 'srv-2',
        name: 'resources-only',
        transport: 'stdio',
        command: 'npx',
        enabled: true,
      },
      status: 'connected',
      runtimeId: 'runtime-2',
      capabilities: { resources: {} },
      tools: [],
      resources: [],
    });

    expect(tools).toEqual([]);
    expect(apiMock.mcpSendRequest).not.toHaveBeenCalled();
  });

  it('closes a spawned stdio runtime when the handshake fails after spawn', async () => {
    const { connectMcpServer } = await import('@/lib/ai/mcp/mcpClient');
    const state = {
      config: {
        id: 'srv-3',
        name: 'broken-stdio',
        transport: 'stdio' as const,
        command: 'npx',
        args: ['broken'],
        enabled: true,
      },
      status: 'connecting' as const,
      tools: [],
      resources: [],
    };

    apiMock.mcpSpawnServer.mockResolvedValue('runtime-broken');
    apiMock.mcpSendRequest.mockImplementation(async (_runtimeId: string, method: string) => {
      if (method === 'initialize') {
        return { capabilities: { tools: {} } };
      }
      if (method === 'notifications/initialized') {
        return null;
      }
      throw new Error('tools/list failed');
    });

    const result = await connectMcpServer(state);

    expect(result.status).toBe('error');
    expect(apiMock.mcpCloseServer).toHaveBeenCalledWith('runtime-broken');
  });

  it('connects legacy SSE servers through endpoint discovery', async () => {
    const { connectMcpServer } = await import('@/lib/ai/mcp/mcpClient');
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (fetchMock.mock.calls.length === 1) {
        expect(url).toBe('http://localhost:3000/mcp/sse');
        expect(init?.method).toBe('GET');
        return new Response('event: endpoint\ndata: /mcp/message\n\n', {
          status: 200,
          headers: { 'Content-Type': 'text/event-stream' },
        });
      }
      if (fetchMock.mock.calls.length === 2) {
        expect(url).toBe('http://localhost:3000/mcp/message');
        expect(init?.method).toBe('POST');
        return new Response(JSON.stringify({ jsonrpc: '2.0', id: 1, result: { capabilities: {} } }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        });
      }
      expect(url).toBe('http://localhost:3000/mcp/message');
      expect(init?.method).toBe('POST');
      return new Response(null, { status: 204 });
    });
    vi.stubGlobal('fetch', fetchMock);

    const result = await connectMcpServer({
      config: {
        id: 'srv-4',
        name: 'sse-server',
        transport: 'legacy-sse',
        url: 'http://localhost:3000/mcp/sse',
        enabled: true,
      },
      status: 'connecting',
      tools: [],
      resources: [],
    });

    expect(result.status).toBe('connected');
    expect(result.resolvedTransport).toBe('legacy-sse');
    expect(result.endpointUrl).toBe('http://localhost:3000/mcp/message');
    expect(fetchMock).toHaveBeenCalledTimes(3);
  });

  it('fails HTTP connection when initialize response is missing result', async () => {
    const { connectMcpServer } = await import('@/lib/ai/mcp/mcpClient');
    const fetchMock = vi.fn(async () => new Response(JSON.stringify({ jsonrpc: '2.0', id: 1 }), {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
    }));
    vi.stubGlobal('fetch', fetchMock);

    const result = await connectMcpServer({
      config: {
        id: 'srv-5',
        name: 'bad-http',
        transport: 'streamable-http',
        url: 'http://localhost:3000/mcp',
        enabled: true,
      },
      status: 'connecting',
      tools: [],
      resources: [],
    });

    expect(result.status).toBe('error');
    expect(result.error).toContain('missing result');
  });

  it('supports streamable HTTP responses returned as text/event-stream', async () => {
    const { connectMcpServer } = await import('@/lib/ai/mcp/mcpClient');
    const fetchMock = vi.fn(async (_input: RequestInfo | URL, init?: RequestInit) => {
      expect(String(_input)).toBe('http://localhost:3000/mcp');
      const body = String(init?.body ?? '');
      const parsed = JSON.parse(body) as { id?: number; method?: string };
      if (body.includes('"method":"initialize"')) {
        return new Response(
          `event: message\ndata: ${JSON.stringify({ jsonrpc: '2.0', id: parsed.id, result: { capabilities: {} } })}\n\n`,
          { status: 200, headers: { 'Content-Type': 'text/event-stream', 'Mcp-Session-Id': 'session-1' } },
        );
      }
      return new Response(null, { status: 202, headers: { 'Mcp-Session-Id': 'session-1' } });
    });
    vi.stubGlobal('fetch', fetchMock);

    const result = await connectMcpServer({
      config: {
        id: 'srv-6',
        name: 'streamable-http',
        transport: 'streamable-http',
        url: 'http://localhost:3000/mcp',
        enabled: true,
      },
      status: 'connecting',
      tools: [],
      resources: [],
    });

    expect(result.status).toBe('connected');
    expect(result.sessionId).toBe('session-1');
    expect(result.endpointUrl).toBe('http://localhost:3000/mcp');
    expect(result.resolvedTransport).toBe('streamable-http');
    expect(fetchMock.mock.calls[0][1]?.headers).toMatchObject({
      Accept: 'application/json, text/event-stream',
      'MCP-Protocol-Version': '2025-11-25',
    });
  });

  it('treats legacy sse config values as streamable HTTP compatibility mode', async () => {
    const { connectMcpServer } = await import('@/lib/ai/mcp/mcpClient');
    const fetchMock = vi.fn(async (_input: RequestInfo | URL, init?: RequestInit) => {
      expect(String(_input)).toBe('http://localhost:3000/mcp');
      if (String(init?.body).includes('"method":"initialize"')) {
        return new Response(JSON.stringify({ jsonrpc: '2.0', id: 1, result: { capabilities: {} } }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        });
      }
      return new Response(null, { status: 202 });
    });
    vi.stubGlobal('fetch', fetchMock);

    const result = await connectMcpServer({
      config: {
        id: 'srv-legacy-name',
        name: 'legacy-name',
        transport: 'sse',
        url: 'http://localhost:3000/mcp',
        enabled: true,
      },
      status: 'connecting',
      tools: [],
      resources: [],
    });

    expect(result.status).toBe('connected');
    expect(result.resolvedTransport).toBe('streamable-http');
  });

  it('falls back to legacy HTTP+SSE endpoint discovery after a 4xx initialize POST', async () => {
    const { connectMcpServer } = await import('@/lib/ai/mcp/mcpClient');
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (init?.method === 'POST' && url === 'http://localhost:3000/legacy/message' && String(init.body).includes('"method":"initialize"')) {
        return new Response(JSON.stringify({ jsonrpc: '2.0', id: 2, result: { capabilities: {} } }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        });
      }
      if (init?.method === 'POST' && url === 'http://localhost:3000/legacy/message') {
        return new Response(null, { status: 202 });
      }
      if (init?.method === 'POST') {
        return new Response('method not allowed', { status: 405, statusText: 'Method Not Allowed' });
      }
      return new Response('event: endpoint\ndata: /legacy/message\n\n', {
        status: 200,
        headers: { 'Content-Type': 'text/event-stream' },
      });
    });
    vi.stubGlobal('fetch', fetchMock);

    const result = await connectMcpServer({
      config: {
        id: 'srv-7',
        name: 'legacy-sse',
        transport: 'streamable-http',
        url: 'http://localhost:3000/sse',
        enabled: true,
      },
      status: 'connecting',
      tools: [],
      resources: [],
    });

    expect(result.status).toBe('connected');
    expect(result.endpointUrl).toBe('http://localhost:3000/legacy/message');
    expect(result.resolvedTransport).toBe('legacy-sse');
    expect(fetchMock.mock.calls.some((call) => String(call[0]) === 'http://localhost:3000/sse' && call[1]?.method === 'GET')).toBe(true);
  });

  it('sends custom raw auth and extra headers for streamable HTTP', async () => {
    apiMock.getAiProviderApiKey.mockResolvedValue('token-123');
    const { connectMcpServer } = await import('@/lib/ai/mcp/mcpClient');
    const fetchMock = vi.fn(async (_input: RequestInfo | URL, init?: RequestInit) => {
      const headers = init?.headers as Record<string, string>;
      expect(headers['X-API-Key']).toBe('token-123');
      expect(headers['X-Workspace']).toBe('prod');
      expect(headers.Authorization).toBeUndefined();
      if (String(init?.body).includes('"method":"initialize"')) {
        return new Response(JSON.stringify({ jsonrpc: '2.0', id: 1, result: { capabilities: {} } }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        });
      }
      return new Response(null, { status: 202 });
    });
    vi.stubGlobal('fetch', fetchMock);

    const result = await connectMcpServer({
      config: {
        id: 'srv-8',
        name: 'custom-auth',
        transport: 'streamable-http',
        url: 'http://localhost:3000/mcp',
        authHeaderName: 'X-API-Key',
        authHeaderMode: 'raw',
        headers: {
          'X-Workspace': 'prod',
          Accept: 'text/plain',
          'MCP-Session-Id': 'bad',
        },
        enabled: true,
      },
      status: 'connecting',
      tools: [],
      resources: [],
    });

    expect(result.status).toBe('connected');
  });
});
