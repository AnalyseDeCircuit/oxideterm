/**
 * FontPreview Component
 * Preview font files (TTF, OTF, WOFF, WOFF2) with sample text
 */

import React, { useState, useEffect, useId } from 'react';
import { useTranslation } from 'react-i18next';
import { Type, Minus, Plus } from 'lucide-react';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { cn } from '../../lib/utils';

interface FontPreviewProps {
  /** Base64 data URL of the font file */
  data: string;
  /** Font file name (for display and unique ID generation) */
  filename: string;
  /** Optional CSS class */
  className?: string;
}

// Sample texts for different scenarios
const SAMPLE_TEXTS = {
  alphabet: 'ABCDEFGHIJKLMNOPQRSTUVWXYZ\nabcdefghijklmnopqrstuvwxyz',
  numbers: '0123456789',
  symbols: '!@#$%^&*()_+-=[]{}|;:\'",.<>?/\\~`',
  pangram_en: 'The quick brown fox jumps over the lazy dog.',
  pangram_zh: '天地玄黄，宇宙洪荒。日月盈昃，辰宿列张。',
  pangram_ja: 'いろはにほへとちりぬるを わかよたれそつねならむ',
  pangram_ko: '키스의 고유조건은 입술끼리 , 만, 만나야 하, 는 것, 이다',
  nerdFont: '       󰊤  󰇘  󱁤           ',
  code: 'fn main() {\n    println!("Hello, 世界!");\n    let x = 42;\n}',
  ligatures: '-> => == != <= >= && || :: ++ -- ** // /* */ <!-- -->',
};

// Preset font sizes
const FONT_SIZES = [12, 14, 16, 18, 20, 24, 28, 32, 40, 48, 64, 72, 96];

export const FontPreview: React.FC<FontPreviewProps> = ({
  data,
  filename,
  className,
}) => {
  const { t } = useTranslation();
  const uniqueId = useId();
  const fontFamilyName = `preview-font-${uniqueId.replace(/:/g, '')}`;
  
  const [fontSize, setFontSize] = useState(32);
  const [customText, setCustomText] = useState('');
  const [fontLoaded, setFontLoaded] = useState(false);
  const [fontError, setFontError] = useState<string | null>(null);

  // Load the font dynamically
  useEffect(() => {
    const loadFont = async () => {
      try {
        setFontLoaded(false);
        setFontError(null);

        // Create a FontFace and load it
        const font = new FontFace(fontFamilyName, `url(${data})`);
        const loadedFont = await font.load();
        
        // Add to document fonts
        document.fonts.add(loadedFont);
        setFontLoaded(true);
      } catch (err) {
        console.error('Failed to load font:', err);
        setFontError(String(err));
      }
    };

    loadFont();

    // Cleanup: remove font when component unmounts
    return () => {
      document.fonts.forEach(font => {
        if (font.family === fontFamilyName) {
          document.fonts.delete(font);
        }
      });
    };
  }, [data, fontFamilyName]);

  const adjustFontSize = (delta: number) => {
    setFontSize(prev => Math.max(8, Math.min(120, prev + delta)));
  };

  if (fontError) {
    return (
      <div className={cn('flex flex-col items-center justify-center py-16 text-zinc-500', className)}>
        <Type className="h-12 w-12 mb-4 opacity-50" />
        <p className="text-sm">{t('fileManager.fontLoadError', 'Failed to load font')}</p>
        <p className="text-xs text-zinc-600 mt-1">{fontError}</p>
      </div>
    );
  }

  if (!fontLoaded) {
    return (
      <div className={cn('flex items-center justify-center py-16', className)}>
        <div className="animate-pulse text-zinc-500">{t('fileManager.loadingFont', 'Loading font...')}</div>
      </div>
    );
  }

  return (
    <div className={cn('flex flex-col h-full', className)}>
      {/* Controls */}
      <div className="flex items-center gap-4 px-4 py-3 border-b border-theme-border bg-zinc-900/50">
        {/* Font size control */}
        <div className="flex items-center gap-2">
          <Button 
            size="icon" 
            variant="ghost" 
            className="h-7 w-7" 
            onClick={() => adjustFontSize(-4)}
          >
            <Minus className="h-3.5 w-3.5" />
          </Button>
          <span className="text-xs text-zinc-400 w-12 text-center">{fontSize}px</span>
          <Button 
            size="icon" 
            variant="ghost" 
            className="h-7 w-7" 
            onClick={() => adjustFontSize(4)}
          >
            <Plus className="h-3.5 w-3.5" />
          </Button>
        </div>

        {/* Preset sizes */}
        <div className="flex items-center gap-1">
          {[16, 24, 32, 48, 72].map(size => (
            <Button
              key={size}
              size="sm"
              variant={fontSize === size ? 'secondary' : 'ghost'}
              className="h-6 px-2 text-xs"
              onClick={() => setFontSize(size)}
            >
              {size}
            </Button>
          ))}
        </div>

        {/* Custom text input */}
        <div className="flex-1 max-w-xs">
          <Input
            type="text"
            value={customText}
            onChange={(e) => setCustomText(e.target.value)}
            placeholder={t('fileManager.customText', 'Type custom text...')}
            className="h-7 text-xs"
          />
        </div>
      </div>

      {/* Preview area */}
      <div className="flex-1 overflow-auto p-6 bg-zinc-950">
        <div 
          className="space-y-8"
          style={{ fontFamily: `"${fontFamilyName}", sans-serif` }}
        >
          {/* Custom text (if provided) */}
          {customText && (
            <div className="space-y-2">
              <h4 className="text-xs text-zinc-500 uppercase tracking-wider font-sans">
                {t('fileManager.customText', 'Custom Text')}
              </h4>
              <p 
                className="text-zinc-100 whitespace-pre-wrap break-words"
                style={{ fontSize: `${fontSize}px`, lineHeight: 1.4 }}
              >
                {customText}
              </p>
            </div>
          )}

          {/* Alphabet */}
          <div className="space-y-2">
            <h4 className="text-xs text-zinc-500 uppercase tracking-wider font-sans">
              {t('fileManager.fontAlphabet', 'Alphabet')}
            </h4>
            <p 
              className="text-zinc-100 whitespace-pre-wrap"
              style={{ fontSize: `${fontSize}px`, lineHeight: 1.4 }}
            >
              {SAMPLE_TEXTS.alphabet}
            </p>
          </div>

          {/* Numbers & Symbols */}
          <div className="space-y-2">
            <h4 className="text-xs text-zinc-500 uppercase tracking-wider font-sans">
              {t('fileManager.fontNumbers', 'Numbers & Symbols')}
            </h4>
            <p 
              className="text-zinc-100"
              style={{ fontSize: `${fontSize}px`, lineHeight: 1.4 }}
            >
              {SAMPLE_TEXTS.numbers}
            </p>
            <p 
              className="text-zinc-100"
              style={{ fontSize: `${Math.max(fontSize * 0.75, 12)}px`, lineHeight: 1.4 }}
            >
              {SAMPLE_TEXTS.symbols}
            </p>
          </div>

          {/* Pangrams */}
          <div className="space-y-2">
            <h4 className="text-xs text-zinc-500 uppercase tracking-wider font-sans">
              {t('fileManager.fontPangram', 'Pangram')}
            </h4>
            <p 
              className="text-zinc-100"
              style={{ fontSize: `${fontSize}px`, lineHeight: 1.4 }}
            >
              {SAMPLE_TEXTS.pangram_en}
            </p>
          </div>

          {/* CJK Characters */}
          <div className="space-y-2">
            <h4 className="text-xs text-zinc-500 uppercase tracking-wider font-sans">
              {t('fileManager.fontCJK', 'CJK Characters')}
            </h4>
            <p 
              className="text-zinc-100"
              style={{ fontSize: `${fontSize}px`, lineHeight: 1.6 }}
            >
              {SAMPLE_TEXTS.pangram_zh}
            </p>
            <p 
              className="text-zinc-300"
              style={{ fontSize: `${Math.max(fontSize * 0.75, 14)}px`, lineHeight: 1.6 }}
            >
              {SAMPLE_TEXTS.pangram_ja}
            </p>
          </div>

          {/* Nerd Font Icons */}
          <div className="space-y-2">
            <h4 className="text-xs text-zinc-500 uppercase tracking-wider font-sans">
              {t('fileManager.fontNerdIcons', 'Nerd Font Icons')}
            </h4>
            <p 
              className="text-zinc-100"
              style={{ fontSize: `${fontSize}px`, lineHeight: 1.4, letterSpacing: '0.15em' }}
            >
              {SAMPLE_TEXTS.nerdFont}
            </p>
          </div>

          {/* Code Sample */}
          <div className="space-y-2">
            <h4 className="text-xs text-zinc-500 uppercase tracking-wider font-sans">
              {t('fileManager.fontCode', 'Code Sample')}
            </h4>
            <pre 
              className="text-zinc-100 bg-zinc-900 p-4 rounded-lg overflow-x-auto"
              style={{ fontSize: `${Math.max(fontSize * 0.75, 12)}px`, lineHeight: 1.6 }}
            >
              {SAMPLE_TEXTS.code}
            </pre>
          </div>

          {/* Ligatures */}
          <div className="space-y-2">
            <h4 className="text-xs text-zinc-500 uppercase tracking-wider font-sans">
              {t('fileManager.fontLigatures', 'Ligatures')}
            </h4>
            <p 
              className="text-zinc-100"
              style={{ 
                fontSize: `${fontSize}px`, 
                lineHeight: 1.4,
                fontFeatureSettings: '"liga" 1, "calt" 1',
              }}
            >
              {SAMPLE_TEXTS.ligatures}
            </p>
          </div>

          {/* All sizes preview */}
          <div className="space-y-2">
            <h4 className="text-xs text-zinc-500 uppercase tracking-wider font-sans">
              {t('fileManager.fontAllSizes', 'Size Comparison')}
            </h4>
            <div className="space-y-1">
              {FONT_SIZES.map(size => (
                <div key={size} className="flex items-baseline gap-3">
                  <span className="text-xs text-zinc-600 w-8 text-right font-sans">{size}</span>
                  <span 
                    className="text-zinc-200"
                    style={{ fontSize: `${size}px` }}
                  >
                    {filename.replace(/\.[^.]+$/, '')} Ag
                  </span>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

export default FontPreview;
