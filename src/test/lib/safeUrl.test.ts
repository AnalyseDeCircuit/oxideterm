import { describe, expect, it } from 'vitest';
import { validateUrl } from '@/lib/safeUrl';

describe('validateUrl', () => {
  describe('safe protocols', () => {
    it('allows http', () => {
      expect(validateUrl('http://example.com')).toEqual({ safe: true });
    });

    it('allows https', () => {
      expect(validateUrl('https://example.com')).toEqual({ safe: true });
    });

    it('allows https with path', () => {
      expect(validateUrl('https://example.com/path?q=1#hash')).toEqual({ safe: true });
    });
  });

  describe('dangerous protocols', () => {
    it('blocks javascript:', () => {
      const result = validateUrl('javascript:alert(1)');
      expect(result.safe).toBe(false);
      expect(result.reason).toContain('javascript:');
    });

    it('blocks data:', () => {
      const result = validateUrl('data:text/html,<script>alert(1)</script>');
      expect(result.safe).toBe(false);
      expect(result.reason).toContain('data:');
    });

    it('blocks file:', () => {
      const result = validateUrl('file:///etc/passwd');
      expect(result.safe).toBe(false);
      expect(result.reason).toContain('file:');
    });

    it('blocks vbscript:', () => {
      const result = validateUrl('vbscript:MsgBox("hi")');
      expect(result.safe).toBe(false);
    });

    it('blocks blob:', () => {
      const result = validateUrl('blob:https://example.com/uuid');
      expect(result.safe).toBe(false);
    });

    it('blocks about:', () => {
      const result = validateUrl('about:blank');
      expect(result.safe).toBe(false);
    });
  });

  describe('unsupported protocols', () => {
    it('blocks ftp:', () => {
      const result = validateUrl('ftp://example.com');
      expect(result.safe).toBe(false);
      expect(result.reason).toContain('Unsupported protocol');
    });

    it('blocks ssh:', () => {
      const result = validateUrl('ssh://user@host');
      expect(result.safe).toBe(false);
    });
  });

  describe('auto-prefix', () => {
    it('auto-adds https for bare domains', () => {
      expect(validateUrl('example.com')).toEqual({ safe: true });
    });

    it('auto-adds https for domains with paths', () => {
      expect(validateUrl('example.com/path')).toEqual({ safe: true });
    });
  });

  describe('invalid URLs', () => {
    it('rejects completely invalid input', () => {
      const result = validateUrl('://invalid');
      expect(result.safe).toBe(false);
    });
  });

  describe('terminal artifacts', () => {
    it('handles embedded newlines', () => {
      expect(validateUrl('https://exam\r\nple.com/path')).toEqual({ safe: true });
    });

    it('handles embedded spaces', () => {
      expect(validateUrl('https://example .com/path')).toEqual({ safe: true });
    });
  });
});
