// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { describe, it, expect } from 'vitest';
import { sanitizeForAi, sanitizeConnectionInfo, sanitizeApiMessages } from '@/lib/ai/contextSanitizer';

// ═══════════════════════════════════════════════════════════════════════════
// sanitizeForAi — should redact secrets
// ═══════════════════════════════════════════════════════════════════════════

describe('sanitizeForAi', () => {
  // ── Edge cases: must NOT corrupt normal data ──────────────────────────

  describe('preserves normal content', () => {
    it('preserves empty string', () => {
      expect(sanitizeForAi('')).toBe('');
    });

    it('preserves null/undefined passthrough', () => {
      expect(sanitizeForAi(null as unknown as string)).toBe(null);
      expect(sanitizeForAi(undefined as unknown as string)).toBe(undefined);
    });

    it('preserves plain prose text', () => {
      const text = 'The quick brown fox jumps over the lazy dog.';
      expect(sanitizeForAi(text)).toBe(text);
    });

    it('preserves normal code without secrets', () => {
      const code = `function add(a: number, b: number): number {
  return a + b;
}
const result = add(1, 2);
console.log(result);`;
      expect(sanitizeForAi(code)).toBe(code);
    });

    it('preserves normal shell commands', () => {
      const cmds = [
        'ls -la /home/user',
        'cd /var/log && tail -f syslog',
        'grep -r "pattern" src/',
        'docker ps --format "table {{.Names}}\\t{{.Status}}"',
        'git log --oneline -10',
        'curl https://example.com/api/health',
        'npm install express',
        'cargo build --release',
      ];
      for (const cmd of cmds) {
        expect(sanitizeForAi(cmd)).toBe(cmd);
      }
    });

    it('preserves normal env vars without secret names', () => {
      const text = 'export PATH=/usr/local/bin:/usr/bin\nexport HOME=/home/user\nexport LANG=en_US.UTF-8';
      expect(sanitizeForAi(text)).toBe(text);
    });

    it('preserves short values (< 8 chars) even with secret-ish names', () => {
      // Short values are likely type hints, not actual secrets
      const text = 'KEY=abc';
      expect(sanitizeForAi(text)).toBe(text);
    });

    it('preserves normal assignments without secret keywords', () => {
      const text = 'PORT=3000\nHOST=localhost\nDEBUG=true\nNODE_ENV=production';
      expect(sanitizeForAi(text)).toBe(text);
    });

    it('preserves normal URLs without embedded credentials', () => {
      const urls = [
        'https://github.com/user/repo',
        'postgres://localhost:5432/mydb',
        'redis://cache.local:6379',
        'http://api.example.com/v1/users',
      ];
      for (const url of urls) {
        expect(sanitizeForAi(url)).toBe(url);
      }
    });

    it('preserves base64 content that is only lowercase', () => {
      // All lowercase — not a token
      const text = 'abcdefghijklmnopqrstuvwxyzabcdefghijklmnop';
      expect(sanitizeForAi(text)).toBe(text);
    });

    it('preserves base64 content that is only uppercase', () => {
      // All uppercase — not a token
      const text = 'ABCDEFGHIJKLMNOPQRSTUVWXYZABCDEFGHIJKLMNOP';
      expect(sanitizeForAi(text)).toBe(text);
    });

    it('preserves normal file paths', () => {
      const paths = [
        '/Users/developer/Projects/my-app/src/index.ts',
        'C:\\Users\\dev\\Documents\\project\\main.rs',
        '~/.ssh/config',
        '../relative/path/to/file.json',
      ];
      for (const p of paths) {
        expect(sanitizeForAi(p)).toBe(p);
      }
    });

    it('preserves terminal prompts and PS1 strings', () => {
      const prompts = [
        'user@hostname:~$',
        '[root@server /var/log]#',
        '(venv) developer@machine:~/project$',
      ];
      for (const p of prompts) {
        expect(sanitizeForAi(p)).toBe(p);
      }
    });

    it('preserves error messages and stack traces', () => {
      const trace = `Error: ENOENT: no such file or directory, open '/tmp/missing.txt'
    at Object.openSync (node:fs:600:3)
    at Object.readFileSync (node:fs:468:35)
    at main (/app/src/index.ts:42:18)`;
      expect(sanitizeForAi(trace)).toBe(trace);
    });

    it('preserves JSON output without secrets', () => {
      const json = '{"name": "test", "version": "1.0.0", "port": 8080, "debug": true}';
      expect(sanitizeForAi(json)).toBe(json);
    });

    it('preserves git diff output', () => {
      const diff = `diff --git a/src/main.ts b/src/main.ts
index abc1234..def5678 100644
--- a/src/main.ts
+++ b/src/main.ts
@@ -10,3 +10,4 @@ function main() {
   console.log("hello");
+  console.log("world");
 }`;
      expect(sanitizeForAi(diff)).toBe(diff);
    });

    it('preserves table-formatted output', () => {
      const table = `NAME        STATUS   AGE
nginx       Running  3d
redis       Running  5d
postgres    Running  12h`;
      expect(sanitizeForAi(table)).toBe(table);
    });

    it('preserves package.json dependencies', () => {
      const deps = `"dependencies": {
    "react": "^19.0.0",
    "typescript": "~5.8.0",
    "zustand": "^5.0.3"
  }`;
      expect(sanitizeForAi(deps)).toBe(deps);
    });

    it('preserves Cargo.toml content', () => {
      const cargo = `[dependencies]
tokio = { version = "1.40", features = ["full"] }
serde = { version = "1", features = ["derive"] }`;
      expect(sanitizeForAi(cargo)).toBe(cargo);
    });

    it('preserves SQL queries', () => {
      const sql = "SELECT id, name, email FROM users WHERE status = 'active' ORDER BY created_at DESC LIMIT 10;";
      expect(sanitizeForAi(sql)).toBe(sql);
    });
  });

  // ── Must REDACT actual secrets ────────────────────────────────────────

  describe('redacts secrets', () => {
    it('redacts export SECRET_KEY=...', () => {
      const input = 'export SECRET_KEY=my_super_secret_value_123';
      const result = sanitizeForAi(input);
      expect(result).toContain('SECRET_KEY');
      expect(result).toContain('[REDACTED]');
      expect(result).not.toContain('my_super_secret_value_123');
    });

    it('redacts export API_TOKEN=...', () => {
      const input = 'export API_TOKEN=ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx';
      const result = sanitizeForAi(input);
      expect(result).toContain('[REDACTED]');
      expect(result).not.toContain('ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx');
    });

    it('redacts AWS_SECRET_ACCESS_KEY=value', () => {
      const input = 'AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY';
      const result = sanitizeForAi(input);
      expect(result).toContain('[REDACTED]');
      expect(result).not.toContain('wJalrXUtnFEMI');
    });

    it('redacts DB_PASSWORD=...', () => {
      const input = 'DB_PASSWORD=supersecretdbpass123!';
      const result = sanitizeForAi(input);
      expect(result).toContain('DB_PASSWORD');
      expect(result).toContain('[REDACTED]');
      expect(result).not.toContain('supersecretdbpass123');
    });

    it('redacts Authorization: Bearer token', () => {
      const input = 'Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.xxx.yyy';
      const result = sanitizeForAi(input);
      expect(result).toContain('Authorization');
      expect(result).toContain('Bearer');
      expect(result).toContain('[REDACTED]');
      expect(result).not.toContain('eyJhbGciOi');
    });

    it('redacts AWS AKIA access keys', () => {
      const input = 'Access key: AKIAIOSFODNN7EXAMPLE';
      const result = sanitizeForAi(input);
      expect(result).toContain('[REDACTED]');
      expect(result).not.toContain('AKIAIOSFODNN7EXAMPLE');
    });

    it('redacts PEM private key blocks', () => {
      const input = `-----BEGIN RSA PRIVATE KEY-----
MIIEowIBAAKCAQEA0Z3VS5JJcds3xfn/ygWyF8PbnGcY5unA67hq6FYsQ
base64contenthere==
-----END RSA PRIVATE KEY-----`;
      const result = sanitizeForAi(input);
      expect(result).toContain('-----BEGIN PRIVATE KEY-----');
      expect(result).toContain('[REDACTED]');
      expect(result).not.toContain('MIIEowIBAAKCAQEA');
    });

    it('redacts OPENSSH private key blocks', () => {
      const input = `-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAA
-----END OPENSSH PRIVATE KEY-----`;
      const result = sanitizeForAi(input);
      expect(result).toContain('[REDACTED]');
      expect(result).not.toContain('b3BlbnNzaC1rZXktdjE');
    });

    it('redacts postgres connection string with password', () => {
      const input = 'postgres://admin:s3cretP@ss@db.example.com:5432/mydb';
      const result = sanitizeForAi(input);
      expect(result).toContain('[REDACTED]');
      expect(result).not.toContain('s3cretP@ss');
      expect(result).toContain('db.example.com');
    });

    it('redacts mongodb connection string with password', () => {
      const input = 'mongodb://root:MyPassword123@mongo.internal:27017/admin';
      const result = sanitizeForAi(input);
      expect(result).toContain('[REDACTED]');
      expect(result).not.toContain('MyPassword123');
    });

    it('redacts multiple secrets in one block', () => {
      const input = `export AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE
export AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY
export DB_PASSWORD=hunter2hunter2h
Authorization: Bearer sk-1234567890abcdefghij`;
      const result = sanitizeForAi(input);
      expect(result).not.toContain('AKIAIOSFODNN7EXAMPLE');
      expect(result).not.toContain('wJalrXUtnFEMI');
      expect(result).not.toContain('hunter2hunter2h');
      expect(result).not.toContain('sk-1234567890');
    });

    it('redacts secrets inside JSON', () => {
      const input = '{"api_key": "sk_live_' + 'abcdefghijklmnopqrstuvwxyz1234567890"}';
      const result = sanitizeForAi(input);
      expect(result).toContain('[REDACTED]');
      // The value should be redacted since it matches KEY= pattern
    });

    it('redacts .env file content', () => {
      const input = `# Database
DATABASE_PASSWORD=my_db_password_here
REDIS_AUTH_TOKEN=redis_token_value_12345`;
      const result = sanitizeForAi(input);
      expect(result).not.toContain('my_db_password_here');
      expect(result).not.toContain('redis_token_value_12345');
    });
  });

  // ── Idempotency ───────────────────────────────────────────────────────

  describe('idempotency', () => {
    it('double sanitization produces same result', () => {
      const input = 'export SECRET_KEY=my_secret_value_here';
      const once = sanitizeForAi(input);
      const twice = sanitizeForAi(once);
      expect(twice).toBe(once);
    });

    it('already-redacted text is unchanged', () => {
      const input = 'DB_PASSWORD=[REDACTED]';
      expect(sanitizeForAi(input)).toBe(input);
    });
  });

  // ── Variable name confusion: type defs must NOT be redacted ───────────

  describe('variable name confusion', () => {
    it('preserves TypeScript type aliases with secret-ish names', () => {
      const inputs = [
        'type AuthToken = string;',
        'type ApiKey = string | undefined;',
        'type SecretKey = { value: string };',
        'let token: string;',
        'const password: string = "";',
      ];
      for (const input of inputs) {
        expect(sanitizeForAi(input)).toBe(input);
      }
    });

    it('preserves interface definitions with secret-ish field names', () => {
      const input = `interface Config {
  token: string;
  apiKey: string;
  password: string;
  secretKey?: Buffer;
}`;
      expect(sanitizeForAi(input)).toBe(input);
    });

    it('preserves function parameter names', () => {
      const input = 'function authenticate(token: string, password: string): boolean {';
      expect(sanitizeForAi(input)).toBe(input);
    });

    it('preserves comments mentioning secret keywords', () => {
      const input = '// The auth_token is validated against the keystore';
      expect(sanitizeForAi(input)).toBe(input);
    });
  });

  // ── Special characters in passwords ───────────────────────────────────

  describe('special chars in passwords', () => {
    it('fully redacts password with $, #, %, ^ characters', () => {
      const input = 'DB_PASSWORD=My$uper#Secret%Pass^123';
      const result = sanitizeForAi(input);
      expect(result).toContain('DB_PASSWORD');
      expect(result).toContain('[REDACTED]');
      expect(result).not.toContain('My$uper');
      expect(result).not.toContain('Secret%Pass');
      expect(result).not.toContain('^123');
    });

    it('fully redacts password with spaces in quotes', () => {
      const input = "DB_PASSWORD='My Super Secret Pass 123'";
      const result = sanitizeForAi(input);
      expect(result).toContain('[REDACTED]');
      expect(result).not.toContain('My Super Secret');
    });

    it('redacts password with unicode characters', () => {
      const input = 'AUTH_TOKEN=pässwörd_tökên_12345';
      const result = sanitizeForAi(input);
      expect(result).toContain('[REDACTED]');
    });
  });

  // ── Vendor-specific token formats ─────────────────────────────────────

  describe('vendor-specific tokens', () => {
    it('redacts GitHub fine-grained PAT (github_pat_...)', () => {
      const input = 'token: github_pat_11ABCDEF0123456789_aBcDeFgHiJkLmNoPqRsTuVwXyZ0123456789abcdefghijklmnop';
      const result = sanitizeForAi(input);
      expect(result).toContain('[REDACTED]');
      expect(result).not.toContain('github_pat_11ABCDEF');
    });

    it('redacts GitHub classic PAT (ghp_...)', () => {
      const input = 'GITHUB_TOKEN=ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefgh';
      const result = sanitizeForAi(input);
      expect(result).toContain('[REDACTED]');
      expect(result).not.toContain('ghp_ABCDEFG');
    });

    it('redacts OpenAI API key (sk-proj-...)', () => {
      const input = 'OPENAI_API_KEY=sk-proj-abcdefghijklmnopqrstuvwxyz123456';
      const result = sanitizeForAi(input);
      expect(result).toContain('[REDACTED]');
      expect(result).not.toContain('sk-proj-');
    });

    it('redacts Stripe secret key (sk_live_...)', () => {
      const input = 'STRIPE_SECRET_KEY=' + 'sk_live_' + '51HG7dKJf8sE3RmZabc123def456ghi';
      const result = sanitizeForAi(input);
      expect(result).toContain('[REDACTED]');
      expect(result).not.toContain('sk_live_' + '51HG7d');
    });

    it('redacts Stripe test key (sk_test_...)', () => {
      const input = 'stripe_key=' + 'sk_test_' + '4eC39HqLyjWDarjtT1zdp7dc';
      const result = sanitizeForAi(input);
      expect(result).toContain('[REDACTED]');
      expect(result).not.toContain('sk_test_' + '4eC39');
    });

    it('redacts standalone vendor tokens without KEY= prefix', () => {
      const input = 'Using token github_pat_11ABCDEF0123456789_aBcDeFgHiJkLmNoPqRsTuVwXyZabcdef1234567890abcdefghijk';
      const result = sanitizeForAi(input);
      expect(result).toContain('[REDACTED]');
      expect(result).not.toContain('github_pat_11ABCDEF');
    });
  });

  // ── JSON escaping & compact formats ───────────────────────────────────

  describe('JSON escaping', () => {
    it('redacts JSON without spaces: {"api_key":"value"}', () => {
      const input = '{"api_key":"sk-1234567890abcdef"}';
      const result = sanitizeForAi(input);
      expect(result).toContain('[REDACTED]');
      expect(result).not.toContain('sk-1234567890');
    });

    it('redacts JSON with single-quoted values (non-standard)', () => {
      // Some configs use single quotes
      const input = "{'password':'my_super_password_1234'}";
      const result = sanitizeForAi(input);
      expect(result).toContain('[REDACTED]');
      expect(result).not.toContain('my_super_password');
    });

    it('redacts nested JSON with secrets', () => {
      const input = '{"config":{"database":{"password":"dbPass12345678"}}}';
      const result = sanitizeForAi(input);
      expect(result).toContain('[REDACTED]');
      expect(result).not.toContain('dbPass12345678');
    });

    it('preserves JSON with short values (non-secret)', () => {
      const input = '{"token":"abc","password":"short"}';
      // Short values (< 8 chars) should be kept — likely placeholders
      expect(sanitizeForAi(input)).toBe(input);
    });
  });

  // ── Mixed content: secrets among normal text ──────────────────────────

  describe('mixed content', () => {
    it('preserves normal lines while redacting secret lines', () => {
      const input = `$ cat .env
PORT=3000
HOST=localhost
DB_PASSWORD=supersecretpass1
NODE_ENV=production`;
      const result = sanitizeForAi(input);
      expect(result).toContain('PORT=3000');
      expect(result).toContain('HOST=localhost');
      expect(result).toContain('NODE_ENV=production');
      expect(result).not.toContain('supersecretpass1');
    });

    it('handles terminal output with prompt + commands + secrets', () => {
      const input = `user@server:~$ echo $HOME
/home/user
user@server:~$ export API_KEY=${'sk_live_' + 'abcdefghijklmnopqrstuvwx'}
user@server:~$ curl -H "Authorization: Bearer eyJtoken123456789012345678901234567" https://api.example.com
{"status": "ok"}`;
      const result = sanitizeForAi(input);
      expect(result).toContain('user@server:~$');
      expect(result).toContain('/home/user');
      expect(result).toContain('https://api.example.com');
      expect(result).toContain('{"status": "ok"}');
      // Secrets should be gone
      expect(result).not.toContain('sk_live_' + 'abcdefghijklmnop');
    });

    it('preserves cargo build output', () => {
      const output = `   Compiling serde v1.0.200
   Compiling tokio v1.40.0
   Compiling oxideterm v1.0.13
    Finished release [optimized] target(s) in 42.5s`;
      expect(sanitizeForAi(output)).toBe(output);
    });

    it('preserves npm/pnpm install output', () => {
      const output = `Packages: +142
++++++++++++++++++++++++++++++++++++++++
Progress: resolved 1204, reused 1062, downloaded 0, added 142, done`;
      expect(sanitizeForAi(output)).toBe(output);
    });
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// sanitizeConnectionInfo
// ═══════════════════════════════════════════════════════════════════════════

describe('sanitizeConnectionInfo', () => {
  it('masks username', () => {
    expect(sanitizeConnectionInfo('admin', 'server.example.com', 22)).toBe('****@server.example.com:22');
  });

  it('preserves host and port', () => {
    const result = sanitizeConnectionInfo('root', '192.168.1.100', 2222);
    expect(result).toContain('192.168.1.100');
    expect(result).toContain('2222');
    expect(result).not.toContain('root');
  });

  it('handles empty username', () => {
    expect(sanitizeConnectionInfo('', 'host.local', 22)).toBe('****@host.local:22');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// sanitizeApiMessages — last-mile safety net
// ═══════════════════════════════════════════════════════════════════════════

describe('sanitizeApiMessages', () => {
  it('preserves messages with null content (tool_calls assistant messages)', () => {
    const msgs = [
      { role: 'assistant' as const, content: null, tool_calls: [{ id: '1', type: 'function', function: { name: 'ls', arguments: '{}' } }] },
    ];
    const result = sanitizeApiMessages(msgs);
    expect(result).toEqual(msgs);
    expect(result[0]).toBe(msgs[0]); // same reference — no allocation
  });

  it('preserves messages with undefined content', () => {
    const msgs = [{ role: 'system' as const }];
    const result = sanitizeApiMessages(msgs);
    expect(result[0]).toBe(msgs[0]);
  });

  it('preserves clean messages without allocation', () => {
    const msgs = [
      { role: 'system' as const, content: 'You are a helpful assistant.' },
      { role: 'user' as const, content: 'Hello!' },
    ];
    const result = sanitizeApiMessages(msgs);
    expect(result[0]).toBe(msgs[0]); // same object — no spread
    expect(result[1]).toBe(msgs[1]);
  });

  it('sanitizes content field while preserving other fields', () => {
    const msgs = [
      {
        role: 'user' as const,
        content: 'export SECRET_KEY=abc123secretvalue here',
        id: 'msg-42',
        timestamp: 1234567890,
      },
    ];
    const result = sanitizeApiMessages(msgs);
    expect(result[0].content).toContain('[REDACTED]');
    expect(result[0].content).not.toContain('abc123secretvalue');
    expect(result[0].id).toBe('msg-42');
    expect(result[0].timestamp).toBe(1234567890);
    expect(result[0].role).toBe('user');
  });

  it('handles mixed array: some clean, some need sanitization', () => {
    const msgs = [
      { role: 'system' as const, content: 'You are helpful.' },
      { role: 'user' as const, content: 'DB_PASSWORD=hunter2hunter2hunter2' },
      { role: 'assistant' as const, content: 'I see you have a database configured.' },
      { role: 'tool' as const, content: '{"output": "ok"}' },
    ];
    const result = sanitizeApiMessages(msgs);
    expect(result[0]).toBe(msgs[0]); // untouched
    expect(result[1].content).toContain('[REDACTED]');
    expect(result[1].content).not.toContain('hunter2hunter2hunter2');
    expect(result[2]).toBe(msgs[2]); // untouched
    expect(result[3]).toBe(msgs[3]); // untouched
  });

  it('does not mutate original messages', () => {
    const original = { role: 'user' as const, content: 'export AUTH_TOKEN=secretvalue12345678' };
    const msgs = [original];
    sanitizeApiMessages(msgs);
    expect(original.content).toBe('export AUTH_TOKEN=secretvalue12345678'); // unchanged
  });

  it('returns new array (not same reference)', () => {
    const msgs = [{ role: 'user' as const, content: 'hello' }];
    const result = sanitizeApiMessages(msgs);
    expect(result).not.toBe(msgs); // always new array from .map()
  });
});
