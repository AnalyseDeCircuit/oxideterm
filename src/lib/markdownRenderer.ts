/**
 * High-performance Markdown renderer
 * 
 * Uses marked for parsing and DOMPurify for sanitization.
 * Custom renderer for code blocks with RUN/COPY buttons.
 * KaTeX for math formula rendering (loaded dynamically).
 * 
 * Shared by: AI Chat, QuickLook file preview, AI Inline Panel
 */

import { marked, type Renderer, type Tokens } from 'marked';
import DOMPurify from 'dompurify';
import Prism from 'prismjs';
import '../components/fileManager/prismLanguages';

// ============================================================================
// KaTeX Dynamic Loading (loaded on first math formula encounter)
// ============================================================================
type KaTeXModule = typeof import('katex');
let katexInstance: KaTeXModule | null = null;
let katexLoadPromise: Promise<KaTeXModule> | null = null;
let katexCssLoaded = false;

/**
 * Dynamically load KaTeX library and CSS
 */
async function getKaTeX(): Promise<KaTeXModule> {
  if (katexInstance) return katexInstance;
  
  if (!katexLoadPromise) {
    katexLoadPromise = import('katex').then((module) => {
      katexInstance = module;
      
      // Load KaTeX CSS if not already loaded
      if (!katexCssLoaded) {
        const link = document.createElement('link');
        link.rel = 'stylesheet';
        link.href = 'https://cdn.jsdelivr.net/npm/katex@0.16.28/dist/katex.min.css';
        link.crossOrigin = 'anonymous';
        document.head.appendChild(link);
        katexCssLoaded = true;
      }
      
      return module;
    });
  }
  
  return katexLoadPromise;
}

// ============================================================================
// Language Configuration
// ============================================================================

// Language aliases for normalization
const LANGUAGE_ALIASES: Record<string, string> = {
  'js': 'javascript',
  'ts': 'typescript',
  'py': 'python',
  'rb': 'ruby',
  'sh': 'bash',
  'zsh': 'bash',
  'shell': 'bash',
  'console': 'bash',
  'terminal': 'bash',
  'powershell': 'bash',
  'ps1': 'bash',
  'cmd': 'bash',
  'yml': 'yaml',
  'dockerfile': 'docker',
  'rs': 'rust',
  'kt': 'kotlin',
  'cs': 'csharp',
  'md': 'markdown',
  'htm': 'markup',
  'html': 'markup',
  'xml': 'markup',
  'svg': 'markup',
};

// Shell-like languages that support "RUN" button
const SHELL_LANGUAGES = new Set([
  'bash', 'sh', 'zsh', 'shell', 'console', 'terminal', 
  'powershell', 'ps1', 'cmd', ''
]);

// Mermaid diagram languages
const MERMAID_LANGUAGES = new Set(['mermaid', 'mmd']);

/**
 * Check if language is a Mermaid diagram
 */
export function isMermaidLanguage(lang: string): boolean {
  return MERMAID_LANGUAGES.has(lang.toLowerCase().trim());
}

/**
 * Normalize language identifier
 */
function normalizeLanguage(lang: string): string {
  const lower = lang.toLowerCase().trim();
  return LANGUAGE_ALIASES[lower] || lower;
}

/**
 * Check if language is a shell language
 */
export function isShellLanguage(lang: string): boolean {
  const normalized = normalizeLanguage(lang);
  return SHELL_LANGUAGES.has(normalized) || SHELL_LANGUAGES.has(lang.toLowerCase());
}

/**
 * Apply Prism syntax highlighting
 */
function highlightCode(code: string, lang: string): string {
  const normalized = normalizeLanguage(lang);
  
  // Check if Prism has this language
  if (Prism.languages[normalized]) {
    try {
      return Prism.highlight(code, Prism.languages[normalized], normalized);
    } catch {
      // Fallback to plain text
      return escapeHtml(code);
    }
  }
  
  // Fallback: escape HTML
  return escapeHtml(code);
}

/**
 * Escape HTML entities
 */
function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

/**
 * Generate unique ID for code blocks
 */
let codeBlockCounter = 0;
function generateCodeBlockId(): string {
  return `code-block-${++codeBlockCounter}-${Date.now()}`;
}

/**
 * Generate unique ID for mermaid diagrams
 */
let mermaidCounter = 0;
function generateMermaidId(): string {
  return `mermaid-${++mermaidCounter}-${Date.now()}`;
}

/**
 * Render options for markdown
 */
export interface RenderOptions {
  /** Show RUN button for shell code blocks (default: true) */
  showRunButton?: boolean;
  /** Show COPY button for code blocks (default: true) */
  showCopyButton?: boolean;
}

// Current render options (set before each render)
let currentRenderOptions: RenderOptions = {};

/**
 * Create custom marked renderer
 */
function createRenderer(): Partial<Renderer> {
  return {
    // Code blocks with syntax highlighting and action buttons
    code({ text, lang }: Tokens.Code): string {
      const language = lang || '';
      
      // Handle Mermaid diagrams specially
      if (isMermaidLanguage(language)) {
        const mermaidId = generateMermaidId();
        // Escape for data attribute (base64 encode to avoid escaping issues)
        const encodedSrc = btoa(encodeURIComponent(text));
        return `
          <div class="md-mermaid-container" data-mermaid-id="${mermaidId}">
            <div class="md-mermaid-header">
              <span class="md-mermaid-label">mermaid</span>
              <button class="md-mermaid-zoom-btn" data-action="zoom-mermaid" data-target="${mermaidId}" title="Expand">
                <svg class="md-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7"/>
                </svg>
              </button>
            </div>
            <div class="md-mermaid" data-mermaid-src="${encodedSrc}" id="${mermaidId}">
              <pre class="md-mermaid-fallback">${escapeHtml(text)}</pre>
            </div>
          </div>
        `;
      }
      
      const displayLang = language || 'text';
      const normalized = normalizeLanguage(language);
      const highlighted = highlightCode(text, language);
      const isShell = isShellLanguage(language);
      const blockId = generateCodeBlockId();
      
      // Get current options
      const showRun = currentRenderOptions.showRunButton !== false && isShell;
      const showCopy = currentRenderOptions.showCopyButton !== false;
      
      // Escape code for data attribute
      const escapedCode = text
        .replace(/&/g, '&amp;')
        .replace(/"/g, '&quot;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;');

      return `
        <div class="md-code-block" data-code-id="${blockId}" data-code="${escapedCode}" data-can-run="${isShell}">
          <div class="md-code-header">
            <span class="md-code-lang">${escapeHtml(displayLang)}</span>
            <div class="md-code-actions">
              ${showRun ? `<button class="md-code-btn md-run-btn" data-action="run" data-target="${blockId}"><svg class="md-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M4 17L10 11L4 5M12 19H20"/></svg><span>RUN</span></button>` : ''}
              ${showCopy ? `<button class="md-code-btn md-copy-btn" data-action="copy" data-target="${blockId}"><svg class="md-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg><span>COPY</span></button>` : ''}
            </div>
          </div>
          <pre class="md-code-pre language-${normalized}"><code class="md-code language-${normalized}">${highlighted}</code></pre>
        </div>
      `;
    },

    // Inline code
    codespan({ text }: Tokens.Codespan): string {
      return `<code class="md-inline-code">${escapeHtml(text)}</code>`;
    },

    // Headings
    heading({ tokens, depth }: Tokens.Heading): string {
      const text = this.parser?.parseInline(tokens) || '';
      const classes = [
        'md-heading',
        `md-h${depth}`,
        depth <= 2 ? 'md-heading-major' : 'md-heading-minor'
      ].join(' ');
      return `<h${depth} class="${classes}">${text}</h${depth}>`;
    },

    // Paragraphs
    paragraph({ tokens }: Tokens.Paragraph): string {
      const text = this.parser?.parseInline(tokens) || '';
      return `<p class="md-paragraph">${text}</p>`;
    },

    // Unordered lists
    list({ items, ordered, start }: Tokens.List): string {
      const tag = ordered ? 'ol' : 'ul';
      const startAttr = ordered && start !== 1 ? ` start="${start}"` : '';
      const listClass = ordered ? 'md-list md-list-ordered' : 'md-list md-list-unordered';
      const itemsHtml = items.map(item => this.listitem?.(item) || '').join('');
      return `<${tag} class="${listClass}"${startAttr}>${itemsHtml}</${tag}>`;
    },

    // List items
    listitem({ tokens, task, checked }: Tokens.ListItem): string {
      const text = this.parser?.parse(tokens) || '';
      if (task) {
        const checkClass = checked ? 'md-task-checked' : 'md-task-unchecked';
        const checkIcon = checked ? '☑' : '☐';
        return `<li class="md-list-item md-task-item ${checkClass}"><span class="md-task-checkbox">${checkIcon}</span>${text}</li>`;
      }
      return `<li class="md-list-item">${text}</li>`;
    },

    // Block quotes
    blockquote({ tokens }: Tokens.Blockquote): string {
      const text = this.parser?.parse(tokens) || '';
      return `<blockquote class="md-blockquote">${text}</blockquote>`;
    },

    // Horizontal rule
    hr(): string {
      return '<hr class="md-hr" />';
    },

    // Links - open external in browser, handle file paths
    link({ href, title, tokens }: Tokens.Link): string {
      const text = this.parser?.parseInline(tokens) || '';
      const titleAttr = title ? ` title="${escapeHtml(title)}"` : '';
      const isExternal = href.startsWith('http://') || href.startsWith('https://');
      const isFilePath = href.startsWith('/') || href.startsWith('~') || /^[a-zA-Z]:[\\/]/.test(href);
      
      if (isFilePath) {
        // File path - emit event to open in terminal
        return `<a href="#" class="md-link md-file-link" data-file-path="${escapeHtml(href)}"${titleAttr}>${text}</a>`;
      } else if (isExternal) {
        // External link - open in system browser
        return `<a href="${escapeHtml(href)}" class="md-link md-external-link" target="_blank" rel="noopener noreferrer"${titleAttr}>${text}</a>`;
      } else {
        // Other links
        return `<a href="${escapeHtml(href)}" class="md-link"${titleAttr}>${text}</a>`;
      }
    },

    // Images
    image({ href, title, text }: Tokens.Image): string {
      const titleAttr = title ? ` title="${escapeHtml(title)}"` : '';
      return `<img src="${escapeHtml(href)}" alt="${escapeHtml(text)}" class="md-image"${titleAttr} />`;
    },

    // Strong (bold)
    strong({ tokens }: Tokens.Strong): string {
      const text = this.parser?.parseInline(tokens) || '';
      return `<strong class="md-strong">${text}</strong>`;
    },

    // Emphasis (italic)
    em({ tokens }: Tokens.Em): string {
      const text = this.parser?.parseInline(tokens) || '';
      return `<em class="md-em">${text}</em>`;
    },

    // Strikethrough
    del({ tokens }: Tokens.Del): string {
      const text = this.parser?.parseInline(tokens) || '';
      return `<del class="md-del">${text}</del>`;
    },

    // Tables
    table({ header, rows }: Tokens.Table): string {
      const headerHtml = header.map(cell => {
        const text = this.parser?.parseInline(cell.tokens) || '';
        const align = cell.align ? ` style="text-align: ${cell.align}"` : '';
        return `<th class="md-table-th"${align}>${text}</th>`;
      }).join('');

      const rowsHtml = rows.map(row => {
        const cellsHtml = row.map(cell => {
          const text = this.parser?.parseInline(cell.tokens) || '';
          const align = cell.align ? ` style="text-align: ${cell.align}"` : '';
          return `<td class="md-table-td"${align}>${text}</td>`;
        }).join('');
        return `<tr class="md-table-row">${cellsHtml}</tr>`;
      }).join('');

      return `
        <div class="md-table-wrapper">
          <table class="md-table">
            <thead class="md-table-head"><tr class="md-table-row">${headerHtml}</tr></thead>
            <tbody class="md-table-body">${rowsHtml}</tbody>
          </table>
        </div>
      `;
    },

    // Line break
    br(): string {
      return '<br class="md-br" />';
    },

    // Text - handle raw text (may contain nested inline tokens like strong/em)
    text({ text, tokens }: Tokens.Text): string {
      // If there are nested tokens (e.g., bold inside list item), parse them
      if (tokens && tokens.length > 0) {
        return this.parser?.parseInline(tokens) || text;
      }
      return text;
    },
  };
}

// Configure DOMPurify
function configureDOMPurify(): void {
  // Allow data attributes for our interactive elements
  DOMPurify.addHook('uponSanitizeAttribute', (_node, data) => {
    // Allow our custom data attributes
    if (data.attrName.startsWith('data-')) {
      data.forceKeepAttr = true;
    }
  });
}

// Initialize DOMPurify config
configureDOMPurify();

// Configure marked
const renderer = createRenderer();
marked.use({
  renderer,
  gfm: true,        // GitHub Flavored Markdown
  breaks: false,    // Don't convert \n to <br> (let markdown handle it)
  pedantic: false,
  async: false,
});

/**
 * Parse and sanitize markdown content
 * @param content - The markdown content to render
 * @param options - Render options
 */
export function renderMarkdown(content: string, options: RenderOptions = {}): string {
  // Set current options for the renderer to use
  currentRenderOptions = options;
  
  // Pre-process: protect math formulas from markdown parsing
  const { processed, mathBlocks } = protectMathFormulas(content);
  
  // Parse markdown
  const html = marked.parse(processed, { async: false }) as string;
  
  // Reset options
  currentRenderOptions = {};
  
  // Post-process: restore math formula placeholders with KaTeX markup
  const htmlWithMath = restoreMathFormulas(html, mathBlocks);
  
  // Sanitize HTML
  const clean = DOMPurify.sanitize(htmlWithMath, {
    ALLOWED_TAGS: [
      'div', 'span', 'p', 'br', 'hr',
      'h1', 'h2', 'h3', 'h4', 'h5', 'h6',
      'ul', 'ol', 'li',
      'pre', 'code',
      'blockquote',
      'strong', 'b', 'em', 'i', 'del', 's',
      'a', 'img',
      'table', 'thead', 'tbody', 'tr', 'th', 'td',
      'button', 'svg', 'path', 'rect',
      // KaTeX elements
      'math', 'semantics', 'mrow', 'mi', 'mo', 'mn', 'msup', 'msub', 'mfrac',
      'mover', 'munder', 'munderover', 'mtable', 'mtr', 'mtd', 'mtext', 'mspace',
      'annotation', 'annotation-xml',
    ],
    ALLOWED_ATTR: [
      'class', 'id', 'href', 'src', 'alt', 'title', 'target', 'rel',
      'data-code', 'data-code-id', 'data-can-run', 'data-action', 'data-target', 'data-file-path',
      'data-math', 'data-math-display',
      'style', 'start', 'type',
      'viewBox', 'fill', 'stroke', 'stroke-width', 'd', 'x', 'y', 'width', 'height', 'rx',
      // KaTeX attributes
      'encoding', 'xmlns', 'mathvariant', 'stretchy', 'fence', 'separator', 'lspace', 'rspace',
      'columnalign', 'rowspacing', 'columnspacing', 'displaystyle', 'scriptlevel',
    ],
    ADD_ATTR: ['target', 'rel'],
  });
  
  return clean;
}

// ============================================================================
// Math Formula Handling
// ============================================================================

interface MathBlock {
  placeholder: string;
  formula: string;
  isDisplay: boolean; // true for $$...$$ (block), false for $...$ (inline)
}

let mathBlockCounter = 0;

/**
 * Protect math formulas from markdown parsing by replacing them with placeholders
 */
function protectMathFormulas(content: string): { processed: string; mathBlocks: MathBlock[] } {
  const mathBlocks: MathBlock[] = [];
  let processed = content;
  
  // First, handle display math ($$...$$) - must be done before inline math
  // Match $$ at start of line or after whitespace, and $$ at end or before whitespace
  const displayMathRegex = /\$\$([\s\S]*?)\$\$/g;
  processed = processed.replace(displayMathRegex, (_match, formula) => {
    const placeholder = `%%MATH_BLOCK_${++mathBlockCounter}%%`;
    mathBlocks.push({ placeholder, formula: formula.trim(), isDisplay: true });
    return placeholder;
  });
  
  // Then, handle inline math ($...$)
  // Must not match $$ (already replaced) and must have non-space after opening $ and before closing $
  // Also avoid matching things like $10 or prices
  const inlineMathRegex = /(?<!\$)\$(?!\$)([^\s$](?:[^$]*?[^\s$])?)\$(?!\$)/g;
  processed = processed.replace(inlineMathRegex, (_match, formula) => {
    // Skip if it looks like a price (e.g., $10, $5.99)
    if (/^\d/.test(formula)) return _match;
    
    const placeholder = `%%MATH_INLINE_${++mathBlockCounter}%%`;
    mathBlocks.push({ placeholder, formula: formula.trim(), isDisplay: false });
    return placeholder;
  });
  
  return { processed, mathBlocks };
}

/**
 * Restore math formula placeholders with KaTeX-ready markup
 * The actual KaTeX rendering happens in renderMathInElement()
 */
function restoreMathFormulas(html: string, mathBlocks: MathBlock[]): string {
  let result = html;
  
  for (const block of mathBlocks) {
    // Escape HTML entities in the formula for the data attribute
    const escapedFormula = block.formula
      .replace(/&/g, '&amp;')
      .replace(/"/g, '&quot;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;');
    
    if (block.isDisplay) {
      // Display math (block level)
      result = result.replace(
        block.placeholder,
        `<div class="md-math md-math-display" data-math="${escapedFormula}" data-math-display="true">$$${escapeHtml(block.formula)}$$</div>`
      );
    } else {
      // Inline math
      result = result.replace(
        block.placeholder,
        `<span class="md-math md-math-inline" data-math="${escapedFormula}">$${escapeHtml(block.formula)}$</span>`
      );
    }
  }
  
  return result;
}

/**
 * Render all math elements in a container using KaTeX
 * Call this after inserting markdown HTML into the DOM
 */
export async function renderMathInElement(container: HTMLElement): Promise<void> {
  const mathElements = container.querySelectorAll<HTMLElement>('.md-math:not(.rendered)');
  
  if (mathElements.length === 0) return;
  
  // Load KaTeX dynamically
  const katex = await getKaTeX();
  
  for (const element of mathElements) {
    const formula = element.getAttribute('data-math');
    if (!formula) continue;
    
    const isDisplay = element.getAttribute('data-math-display') === 'true';
    
    try {
      // Decode the formula
      const decodedFormula = formula
        .replace(/&amp;/g, '&')
        .replace(/&quot;/g, '"')
        .replace(/&lt;/g, '<')
        .replace(/&gt;/g, '>');
      
      // Render with KaTeX
      const rendered = katex.default.renderToString(decodedFormula, {
        displayMode: isDisplay,
        throwOnError: false,
        errorColor: '#ff6b6b',
        trust: false,
        strict: false,
      });
      
      element.innerHTML = rendered;
      element.classList.add('rendered');
    } catch (error) {
      console.error('KaTeX render error:', error);
      element.classList.add('error');
      // Keep the original LaTeX as fallback
    }
  }
}

/**
 * CSS styles for markdown rendering (to be injected)
 */
export const markdownStyles = `
/* Markdown Base Styles */
.md-content {
  font-size: 13.5px;
  line-height: 1.65;
  color: var(--theme-text-secondary, rgba(255, 255, 255, 0.9));
}

/* Paragraphs */
.md-paragraph {
  margin: 0 0 1em 0;
}
.md-paragraph:last-child {
  margin-bottom: 0;
}

/* Headings */
.md-heading {
  margin: 1.25em 0 0.5em 0;
  font-weight: 600;
  line-height: 1.3;
  color: var(--theme-text, #fff);
}
.md-heading:first-child {
  margin-top: 0;
}
.md-h1 { font-size: 1.5em; }
.md-h2 { font-size: 1.3em; }
.md-h3 { font-size: 1.15em; }
.md-h4 { font-size: 1.05em; }
.md-h5 { font-size: 1em; }
.md-h6 { font-size: 0.95em; opacity: 0.9; }

/* Lists */
.md-list {
  margin: 0.75em 0;
  padding-left: 1.5em;
}
.md-list-unordered {
  list-style-type: disc;
}
.md-list-ordered {
  list-style-type: decimal;
}
.md-list-item {
  margin: 0.25em 0;
  line-height: 1.5;
}
.md-list .md-list {
  margin: 0.25em 0;
}

/* Task lists */
.md-task-item {
  list-style: none;
  margin-left: -1.5em;
  padding-left: 0;
}
.md-task-checkbox {
  margin-right: 0.5em;
  opacity: 0.7;
}
.md-task-checked {
  opacity: 0.7;
  text-decoration: line-through;
}

/* Inline code */
.md-inline-code {
  padding: 0.15em 0.4em;
  margin: 0 0.1em;
  font-size: 0.9em;
  font-family: var(--terminal-font-family, 'JetBrains Mono', monospace);
  background: var(--theme-bg-panel, rgba(255, 255, 255, 0.05));
  border: 1px solid var(--theme-border, rgba(255, 255, 255, 0.1));
  border-radius: 3px;
  color: var(--theme-accent, #22d3ee);
}

/* Code blocks */
.md-code-block {
  margin: 1em 0;
  border-radius: 4px;
  overflow: hidden;
  background: var(--theme-bg-darker, #09090b);
  border: 1px solid var(--theme-border, rgba(255, 255, 255, 0.08));
}

.md-code-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.4em 0.75em;
  background: var(--theme-bg-panel, rgba(255, 255, 255, 0.03));
  border-bottom: 1px solid var(--theme-border, rgba(255, 255, 255, 0.05));
}

.md-code-lang {
  font-size: 10px;
  font-family: var(--terminal-font-family, 'JetBrains Mono', monospace);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--theme-text-muted, rgba(255, 255, 255, 0.4));
  opacity: 0.7;
}

.md-code-actions {
  display: flex;
  align-items: center;
  gap: 0.75em;
}

.md-code-btn {
  display: flex;
  align-items: center;
  gap: 0.35em;
  padding: 0.2em 0;
  background: none;
  border: none;
  cursor: pointer;
  color: var(--theme-text-muted, rgba(255, 255, 255, 0.5));
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.03em;
  transition: color 0.15s ease;
}

.md-code-btn:hover {
  color: var(--theme-text, #fff);
}

.md-run-btn:hover {
  color: var(--theme-accent, #22d3ee);
}

.md-code-btn .md-icon {
  width: 12px;
  height: 12px;
}

.md-code-btn.copied {
  color: #22c55e;
}

.md-code-pre {
  margin: 0;
  padding: 0.75em 1em;
  overflow-x: auto;
  font-size: 13px;
  line-height: 1.5;
}

.md-code {
  font-family: var(--terminal-font-family, 'JetBrains Mono', monospace);
  color: var(--theme-text, #fff);
  display: block;
  white-space: pre;
}

/* Blockquote */
.md-blockquote {
  margin: 1em 0;
  padding: 0.5em 0 0.5em 1em;
  border-left: 3px solid var(--theme-accent, #22d3ee);
  background: var(--theme-bg-panel, rgba(255, 255, 255, 0.02));
  color: var(--theme-text-secondary, rgba(255, 255, 255, 0.8));
}
.md-blockquote p {
  margin: 0;
}

/* Horizontal rule */
.md-hr {
  margin: 1.5em 0;
  border: none;
  border-top: 1px solid var(--theme-border, rgba(255, 255, 255, 0.1));
}

/* Links */
.md-link {
  color: var(--theme-accent, #22d3ee);
  text-decoration: none;
  border-bottom: 1px solid transparent;
  transition: border-color 0.15s ease;
}
.md-link:hover {
  border-bottom-color: var(--theme-accent, #22d3ee);
}
.md-external-link::after {
  content: '↗';
  font-size: 0.75em;
  margin-left: 0.2em;
  opacity: 0.6;
}
.md-file-link {
  font-family: var(--terminal-font-family, 'JetBrains Mono', monospace);
  font-size: 0.95em;
}

/* Images */
.md-image {
  max-width: 100%;
  height: auto;
  border-radius: 4px;
  margin: 0.5em 0;
}

/* Strong & Emphasis */
.md-strong,
strong {
  font-weight: 700 !important;
  color: var(--theme-text, #fff);
}
.md-em,
em {
  font-style: italic;
}
.md-del,
del {
  text-decoration: line-through;
  opacity: 0.7;
}

/* Tables */
.md-table-wrapper {
  margin: 1em 0;
  overflow-x: auto;
}
.md-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.95em;
}
.md-table-th,
.md-table-td {
  padding: 0.5em 0.75em;
  border: 1px solid var(--theme-border, rgba(255, 255, 255, 0.1));
  text-align: left;
}
.md-table-th {
  background: var(--theme-bg-panel, rgba(255, 255, 255, 0.03));
  font-weight: 600;
  color: var(--theme-text, #fff);
}
.md-table-row:nth-child(even) .md-table-td {
  background: var(--theme-bg-panel, rgba(255, 255, 255, 0.01));
}

/* Prism.js syntax highlighting overrides */
.md-code .token.comment,
.md-code .token.prolog,
.md-code .token.doctype,
.md-code .token.cdata {
  color: #6a737d;
}
.md-code .token.punctuation {
  color: #a8b1c2;
}
.md-code .token.property,
.md-code .token.tag,
.md-code .token.boolean,
.md-code .token.number,
.md-code .token.constant,
.md-code .token.symbol,
.md-code .token.deleted {
  color: #f97583;
}
.md-code .token.selector,
.md-code .token.attr-name,
.md-code .token.string,
.md-code .token.char,
.md-code .token.builtin,
.md-code .token.inserted {
  color: #9ecbff;
}
.md-code .token.operator,
.md-code .token.entity,
.md-code .token.url,
.language-css .md-code .token.string,
.style .md-code .token.string {
  color: #79b8ff;
}
.md-code .token.atrule,
.md-code .token.attr-value,
.md-code .token.keyword {
  color: #f97583;
}
.md-code .token.function,
.md-code .token.class-name {
  color: #b392f0;
}
.md-code .token.regex,
.md-code .token.important,
.md-code .token.variable {
  color: #ffab70;
}

/* Mermaid Diagram Styles */
.md-mermaid-container {
  margin: 1em 0;
  border-radius: 4px;
  overflow: hidden;
  background: var(--theme-bg-darker, #09090b);
  border: 1px solid var(--theme-border, rgba(255, 255, 255, 0.08));
}

.md-mermaid-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.4em 0.75em;
  background: var(--theme-bg-panel, rgba(255, 255, 255, 0.03));
  border-bottom: 1px solid var(--theme-border, rgba(255, 255, 255, 0.05));
}

.md-mermaid-label {
  font-size: 10px;
  font-family: var(--terminal-font-family, 'JetBrains Mono', monospace);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--theme-text-muted, rgba(255, 255, 255, 0.4));
  opacity: 0.7;
}

.md-mermaid-zoom-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  padding: 0;
  background: none;
  border: none;
  border-radius: 3px;
  cursor: pointer;
  color: var(--theme-text-muted, rgba(255, 255, 255, 0.5));
  transition: all 0.15s ease;
}

.md-mermaid-zoom-btn:hover {
  color: var(--theme-text, #fff);
  background: var(--theme-bg-panel, rgba(255, 255, 255, 0.1));
}

.md-mermaid-zoom-btn .md-icon {
  width: 14px;
  height: 14px;
}

.md-mermaid {
  padding: 1em;
  overflow-x: auto;
  display: flex;
  justify-content: center;
  align-items: center;
  min-height: 100px;
}

.md-mermaid svg {
  max-width: 100%;
  height: auto;
}

.md-mermaid-fallback {
  margin: 0;
  padding: 0.5em;
  font-size: 12px;
  font-family: var(--terminal-font-family, 'JetBrains Mono', monospace);
  color: var(--theme-text-muted, rgba(255, 255, 255, 0.5));
  white-space: pre-wrap;
  word-break: break-word;
}

.md-mermaid.rendered .md-mermaid-fallback {
  display: none;
}

.md-mermaid-error {
  padding: 0.75em 1em;
  background: rgba(239, 68, 68, 0.1);
  border: 1px solid rgba(239, 68, 68, 0.3);
  border-radius: 4px;
  color: #f87171;
  font-size: 12px;
  font-family: var(--terminal-font-family, 'JetBrains Mono', monospace);
}

/* Mermaid zoom modal */
.md-mermaid-modal {
  position: fixed;
  inset: 0;
  z-index: 9999;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.85);
  backdrop-filter: blur(4px);
}

.md-mermaid-modal-content {
  position: relative;
  max-width: 95vw;
  max-height: 95vh;
  padding: 2em;
  background: var(--theme-bg-panel, #1a1a1a);
  border: 1px solid var(--theme-border, rgba(255, 255, 255, 0.1));
  border-radius: 8px;
  overflow: auto;
}

.md-mermaid-modal-close {
  position: absolute;
  top: 0.5em;
  right: 0.5em;
  width: 32px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--theme-bg-darker, #09090b);
  border: 1px solid var(--theme-border, rgba(255, 255, 255, 0.1));
  border-radius: 4px;
  cursor: pointer;
  color: var(--theme-text-muted, rgba(255, 255, 255, 0.5));
  transition: all 0.15s ease;
}

.md-mermaid-modal-close:hover {
  color: var(--theme-text, #fff);
  background: var(--theme-bg-panel, rgba(255, 255, 255, 0.1));
}

.md-mermaid-modal svg {
  max-width: 100%;
  height: auto;
}

/* ============================================================================
   Math Formula Styles (KaTeX)
   ============================================================================ */

/* Math container */
.md-math {
  font-family: KaTeX_Main, 'Times New Roman', serif;
}

/* Display math (block level) */
.md-math-display {
  display: block;
  margin: 1em 0;
  padding: 0.5em 0;
  overflow-x: auto;
  text-align: center;
}

.md-math-display .katex-display {
  margin: 0;
}

/* Inline math */
.md-math-inline {
  display: inline;
}

/* KaTeX color overrides for dark theme */
.md-math .katex {
  color: var(--theme-text, #e4e4e7);
}

.md-math .katex .mord,
.md-math .katex .mop,
.md-math .katex .mbin,
.md-math .katex .mrel,
.md-math .katex .mopen,
.md-math .katex .mclose,
.md-math .katex .mpunct,
.md-math .katex .minner {
  color: inherit;
}

/* Error state - show raw LaTeX with error styling */
.md-math.error {
  color: var(--theme-text-muted, #a1a1aa);
  font-family: var(--terminal-font-family, monospace);
  font-size: 0.9em;
  background: rgba(255, 107, 107, 0.1);
  padding: 0.1em 0.3em;
  border-radius: 3px;
}

/* Unrendered math placeholder */
.md-math:not(.rendered):not(.error) {
  color: var(--theme-text-muted, #a1a1aa);
  font-family: var(--terminal-font-family, monospace);
  font-size: 0.9em;
}
`;
