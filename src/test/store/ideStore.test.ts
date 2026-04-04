import { describe, expect, it } from 'vitest';
import { extensionToLanguage, detectLanguage } from '@/store/ideStore';

describe('extensionToLanguage', () => {
  it('maps TypeScript extensions', () => {
    expect(extensionToLanguage('ts')).toBe('typescript');
    expect(extensionToLanguage('tsx')).toBe('typescript');
    expect(extensionToLanguage('mts')).toBe('typescript');
  });

  it('maps JavaScript extensions', () => {
    expect(extensionToLanguage('js')).toBe('javascript');
    expect(extensionToLanguage('jsx')).toBe('javascript');
    expect(extensionToLanguage('mjs')).toBe('javascript');
    expect(extensionToLanguage('cjs')).toBe('javascript');
  });

  it('maps Rust', () => {
    expect(extensionToLanguage('rs')).toBe('rust');
  });

  it('maps Python', () => {
    expect(extensionToLanguage('py')).toBe('python');
    expect(extensionToLanguage('pyw')).toBe('python');
    expect(extensionToLanguage('pyi')).toBe('python');
  });

  it('maps shell scripts', () => {
    expect(extensionToLanguage('sh')).toBe('shell');
    expect(extensionToLanguage('bash')).toBe('shell');
    expect(extensionToLanguage('zsh')).toBe('shell');
  });

  it('maps markup languages', () => {
    expect(extensionToLanguage('html')).toBe('html');
    expect(extensionToLanguage('md')).toBe('markdown');
    expect(extensionToLanguage('xml')).toBe('xml');
    expect(extensionToLanguage('svg')).toBe('xml');
  });

  it('maps data formats', () => {
    expect(extensionToLanguage('json')).toBe('json');
    expect(extensionToLanguage('yaml')).toBe('yaml');
    expect(extensionToLanguage('yml')).toBe('yaml');
    expect(extensionToLanguage('toml')).toBe('toml');
  });

  it('is case-insensitive', () => {
    expect(extensionToLanguage('RS')).toBe('rust');
    expect(extensionToLanguage('PY')).toBe('python');
  });

  it('returns plaintext for unknown extensions', () => {
    expect(extensionToLanguage('xyz')).toBe('plaintext');
    expect(extensionToLanguage('')).toBe('plaintext');
  });
});

describe('detectLanguage', () => {
  describe('exact filename matching', () => {
    it('detects Makefile', () => {
      expect(detectLanguage('Makefile')).toBe('shell');
      expect(detectLanguage('GNUmakefile')).toBe('shell');
    });

    it('detects Dockerfile', () => {
      expect(detectLanguage('Dockerfile')).toBe('dockerfile');
      expect(detectLanguage('Containerfile')).toBe('dockerfile');
    });

    it('detects Ruby project files', () => {
      expect(detectLanguage('Gemfile')).toBe('ruby');
      expect(detectLanguage('Rakefile')).toBe('ruby');
    });

    it('detects Go module files', () => {
      expect(detectLanguage('go.mod')).toBe('go');
      expect(detectLanguage('go.sum')).toBe('go');
    });

    it('detects lock files', () => {
      expect(detectLanguage('Cargo.lock')).toBe('toml');
      expect(detectLanguage('package-lock.json')).toBe('json');
      expect(detectLanguage('pnpm-lock.yaml')).toBe('yaml');
    });

    it('detects dotfile configs', () => {
      expect(detectLanguage('.gitignore')).toBe('shell');
      expect(detectLanguage('.dockerignore')).toBe('shell');
      expect(detectLanguage('.editorconfig')).toBe('toml');
    });

    it('case-insensitive filename matching', () => {
      expect(detectLanguage('makefile')).toBe('shell');
      expect(detectLanguage('DOCKERFILE')).toBe('dockerfile');
    });
  });

  describe('dotfile patterns', () => {
    it('detects dotfiles with known extensions', () => {
      expect(detectLanguage('.eslintrc.json')).toBe('json');
      expect(detectLanguage('.prettierrc.yaml')).toBe('yaml');
    });

    it('detects shell profile files', () => {
      expect(detectLanguage('.bashrc')).toBe('shell');
      expect(detectLanguage('.zshrc')).toBe('shell');
      expect(detectLanguage('.bash_profile')).toBe('shell');
      expect(detectLanguage('.profile')).toBe('shell');
    });

    it('detects .conf files as shell', () => {
      expect(detectLanguage('.tmux.conf')).toBe('shell');
    });

    it('detects .gitconfig as toml', () => {
      expect(detectLanguage('.gitconfig')).toBe('toml');
    });
  });

  describe('extension fallback', () => {
    it('uses extension for regular files', () => {
      expect(detectLanguage('main.rs')).toBe('rust');
      expect(detectLanguage('app.tsx')).toBe('typescript');
      expect(detectLanguage('style.css')).toBe('css');
    });

    it('handles files with multiple dots', () => {
      expect(detectLanguage('module.config.ts')).toBe('typescript');
      expect(detectLanguage('data.test.json')).toBe('json');
    });

    it('returns plaintext for unknown files', () => {
      expect(detectLanguage('README')).toBe('plaintext');
      expect(detectLanguage('LICENSE')).toBe('plaintext');
    });
  });
});
