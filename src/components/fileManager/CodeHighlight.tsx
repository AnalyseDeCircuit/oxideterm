/**
 * CodeHighlight Component
 * Provides syntax highlighting using PrismJS with CSS counter-based line numbers
 */

import React, { useMemo } from 'react';
import Prism from 'prismjs';
import { getPrismLanguage } from './utils';

// Import Prism core styles (we override most with Tailwind)
import 'prismjs/themes/prism-tomorrow.css';

// Import commonly used languages
// NOTE: Order matters! Some languages have dependencies:
// - markup must come before markup-templating
// - markup-templating must come before PHP
import 'prismjs/components/prism-markup'; // HTML/XML - base for many languages
import 'prismjs/components/prism-css';
import 'prismjs/components/prism-clike'; // Base for C-like languages
import 'prismjs/components/prism-javascript';
import 'prismjs/components/prism-typescript';
import 'prismjs/components/prism-jsx';
import 'prismjs/components/prism-tsx';
import 'prismjs/components/prism-c';
import 'prismjs/components/prism-cpp';
import 'prismjs/components/prism-csharp';
import 'prismjs/components/prism-java';
import 'prismjs/components/prism-markup-templating'; // Required by PHP
import 'prismjs/components/prism-php';
import 'prismjs/components/prism-python';
import 'prismjs/components/prism-rust';
import 'prismjs/components/prism-go';
import 'prismjs/components/prism-ruby';
import 'prismjs/components/prism-swift';
import 'prismjs/components/prism-kotlin';
import 'prismjs/components/prism-scala';
import 'prismjs/components/prism-bash';
import 'prismjs/components/prism-shell-session';
import 'prismjs/components/prism-sql';
import 'prismjs/components/prism-json';
import 'prismjs/components/prism-yaml';
import 'prismjs/components/prism-toml';
import 'prismjs/components/prism-markdown';
import 'prismjs/components/prism-scss';
import 'prismjs/components/prism-sass';
import 'prismjs/components/prism-less';
import 'prismjs/components/prism-diff';
import 'prismjs/components/prism-docker';
import 'prismjs/components/prism-nginx';
import 'prismjs/components/prism-ini';
import 'prismjs/components/prism-git';
import 'prismjs/components/prism-makefile';
import 'prismjs/components/prism-lua';
import 'prismjs/components/prism-perl';
import 'prismjs/components/prism-regex';
import 'prismjs/components/prism-vim';

import { useSettingsStore } from '../../store/settingsStore';

// Get font family CSS value from settings key
const getFontFamilyCSS = (val: string): string => {
  switch(val) {
    case 'jetbrains': return '"JetBrains Mono", monospace';
    case 'meslo': return '"MesloLGM Nerd Font", monospace';
    case 'tinos': return '"Tinos Nerd Font", monospace';
    case 'menlo': return 'Menlo, Monaco, "Courier New", monospace';
    case 'courier': return '"Courier New", Courier, monospace';
    default: return '"JetBrains Mono", monospace';
  }
};

export interface CodeHighlightProps {
  code: string;
  language?: string;
  filename?: string;
  showLineNumbers?: boolean;
  maxLines?: number;
  className?: string;
}

export const CodeHighlight: React.FC<CodeHighlightProps> = ({
  code,
  language,
  filename,
  showLineNumbers = true,
  maxLines,
  className,
}) => {
  // Get font settings from store
  const fontFamily = useSettingsStore(s => s.settings.terminal.fontFamily);
  const fontSize = useSettingsStore(s => s.settings.terminal.fontSize);
  
  // Determine the Prism language from extension or filename
  const prismLanguage = useMemo(() => {
    if (language) return language;
    if (filename) {
      const ext = filename.includes('.') 
        ? filename.substring(filename.lastIndexOf('.') + 1).toLowerCase()
        : '';
      return getPrismLanguage(ext, filename);
    }
    return 'text';
  }, [language, filename]);

  // Optionally truncate lines
  const displayCode = useMemo(() => {
    if (!maxLines) return code;
    const lines = code.split('\n');
    if (lines.length <= maxLines) return code;
    return lines.slice(0, maxLines).join('\n') + '\n// ... truncated';
  }, [code, maxLines]);

  // Get highlighted HTML using Prism.highlight() - this doesn't touch the DOM
  const highlightedLines = useMemo(() => {
    const grammar = Prism.languages[prismLanguage];
    const lines = displayCode.split('\n');
    
    return lines.map(line => {
      if (grammar) {
        try {
          return Prism.highlight(line || ' ', grammar, prismLanguage);
        } catch {
          // Fallback to plain text if highlighting fails
          return escapeHtml(line || ' ');
        }
      }
      return escapeHtml(line || ' ');
    });
  }, [displayCode, prismLanguage]);

  const lineCount = highlightedLines.length;
  const gutterWidth = Math.max(lineCount.toString().length, 2);

  return (
    <div className={`code-highlight-container ${className || ''}`}>
      <pre
        className="overflow-auto m-0 p-0 bg-transparent"
        style={{
          tabSize: 4,
          fontFamily: getFontFamilyCSS(fontFamily),
          fontSize: `${fontSize}px`,
          lineHeight: 1.5,
        }}
      >
        <code
          className={`language-${prismLanguage} block`}
          style={{
            fontFamily: 'inherit',
            fontSize: 'inherit',
          }}
        >
          {highlightedLines.map((lineHtml, index) => (
            <div
              key={index}
              className="code-line flex"
              style={{ minHeight: '1.4em' }}
            >
              {showLineNumbers && (
                <span
                  className="line-number flex-shrink-0 text-right select-none pr-3"
                  style={{
                    width: `${gutterWidth + 1}ch`,
                    color: 'rgba(255, 255, 255, 0.3)',
                  }}
                >
                  {index + 1}
                </span>
              )}
              <span
                className="line-content flex-1"
                style={{ whiteSpace: 'pre' }}
                dangerouslySetInnerHTML={{ __html: lineHtml }}
              />
            </div>
          ))}
        </code>
      </pre>
    </div>
  );
};

// Escape HTML entities
function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}

export default CodeHighlight;
