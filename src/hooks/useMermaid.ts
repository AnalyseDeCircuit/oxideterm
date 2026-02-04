/**
 * useMermaid Hook
 * 
 * Handles Mermaid diagram rendering with automatic re-render on content change.
 * Supports dark theme and responsive sizing.
 */

import { useEffect, useCallback, useRef } from 'react';
import mermaid from 'mermaid';

// Track if mermaid has been initialized
let mermaidInitialized = false;

/**
 * Initialize Mermaid with OxideTerm dark theme
 */
function initMermaid(): void {
  if (mermaidInitialized) return;
  
  mermaid.initialize({
    startOnLoad: false,
    theme: 'dark',
    darkMode: true,
    securityLevel: 'loose',
    fontFamily: 'var(--terminal-font-family, "JetBrains Mono", monospace)',
    fontSize: 14,
    
    // Theme variables matching OxideTerm
    themeVariables: {
      // Background colors
      background: '#09090b',
      primaryColor: '#1e3a5f',
      secondaryColor: '#2d2d2d',
      tertiaryColor: '#1a1a1a',
      
      // Text colors
      primaryTextColor: '#e4e4e7',
      secondaryTextColor: '#a1a1aa',
      tertiaryTextColor: '#71717a',
      
      // Border and line colors
      primaryBorderColor: '#3f3f46',
      secondaryBorderColor: '#27272a',
      lineColor: '#52525b',
      
      // Node colors
      mainBkg: '#18181b',
      nodeBorder: '#3f3f46',
      clusterBkg: '#1f1f23',
      clusterBorder: '#3f3f46',
      
      // Accent color (matches theme-accent)
      activeActorBorderColor: '#22d3ee',
      activationBorderColor: '#22d3ee',
      
      // Flowchart specific
      edgeLabelBackground: '#18181b',
      
      // Sequence diagram
      actorBkg: '#1e3a5f',
      actorBorder: '#3f3f46',
      actorTextColor: '#e4e4e7',
      actorLineColor: '#52525b',
      signalColor: '#e4e4e7',
      signalTextColor: '#e4e4e7',
      labelBoxBkgColor: '#18181b',
      labelBoxBorderColor: '#3f3f46',
      labelTextColor: '#e4e4e7',
      loopTextColor: '#a1a1aa',
      noteBorderColor: '#3f3f46',
      noteBkgColor: '#1a1a1a',
      noteTextColor: '#a1a1aa',
      
      // Git graph
      git0: '#22d3ee',
      git1: '#a78bfa',
      git2: '#34d399',
      git3: '#fb923c',
      git4: '#f472b6',
      git5: '#60a5fa',
      git6: '#facc15',
      git7: '#f87171',
      gitBranchLabel0: '#e4e4e7',
      gitBranchLabel1: '#e4e4e7',
      gitBranchLabel2: '#e4e4e7',
      gitBranchLabel3: '#e4e4e7',
      
      // Pie chart
      pie1: '#22d3ee',
      pie2: '#a78bfa',
      pie3: '#34d399',
      pie4: '#fb923c',
      pie5: '#f472b6',
      pie6: '#60a5fa',
      pie7: '#facc15',
      pie8: '#f87171',
      pie9: '#818cf8',
      pie10: '#2dd4bf',
      pie11: '#fbbf24',
      pie12: '#f97316',
      pieStrokeColor: '#3f3f46',
      pieTitleTextColor: '#e4e4e7',
      pieSectionTextColor: '#e4e4e7',
      pieLegendTextColor: '#a1a1aa',
    },
    
    // Flowchart config
    flowchart: {
      htmlLabels: true,
      curve: 'basis',
      padding: 15,
      nodeSpacing: 50,
      rankSpacing: 50,
    },
    
    // Sequence diagram config
    sequence: {
      diagramMarginX: 50,
      diagramMarginY: 10,
      actorMargin: 50,
      width: 150,
      height: 65,
      boxMargin: 10,
      boxTextMargin: 5,
      noteMargin: 10,
      messageMargin: 35,
    },
  });
  
  mermaidInitialized = true;
}

/**
 * Render a single Mermaid diagram
 */
async function renderDiagram(element: HTMLElement): Promise<void> {
  const encodedSrc = element.getAttribute('data-mermaid-src');
  if (!encodedSrc) return;
  
  try {
    // Decode the source
    const source = decodeURIComponent(atob(encodedSrc));
    const id = element.id || `mermaid-${Date.now()}`;
    
    // Render the diagram
    const { svg } = await mermaid.render(id + '-svg', source);
    
    // Insert the SVG and mark as rendered
    element.innerHTML = svg;
    element.classList.add('rendered');
  } catch (error) {
    console.error('Mermaid render error:', error);
    
    // Show error message
    const errorMessage = error instanceof Error ? error.message : 'Failed to render diagram';
    element.innerHTML = `<div class="md-mermaid-error">⚠️ ${errorMessage}</div>`;
    element.classList.add('error');
  }
}

/**
 * Hook to handle Mermaid rendering in a container
 * 
 * @param containerRef - Ref to the container element
 * @param content - The markdown content (used as dependency for re-render)
 */
export function useMermaid(
  containerRef: React.RefObject<HTMLElement | null>,
  content: string
): void {
  const renderingRef = useRef(false);
  
  const renderAll = useCallback(async () => {
    const container = containerRef.current;
    if (!container || renderingRef.current) return;
    
    // Find all unrendered mermaid diagrams
    const diagrams = container.querySelectorAll<HTMLElement>(
      '.md-mermaid:not(.rendered):not(.error)'
    );
    
    if (diagrams.length === 0) return;
    
    renderingRef.current = true;
    initMermaid();
    
    // Render each diagram
    for (const diagram of diagrams) {
      await renderDiagram(diagram);
    }
    
    renderingRef.current = false;
  }, [containerRef]);
  
  // Render on mount and content change
  useEffect(() => {
    // Small delay to ensure DOM is ready
    const timer = setTimeout(renderAll, 50);
    return () => clearTimeout(timer);
  }, [content, renderAll]);
  
  // Handle zoom button clicks
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    
    const handleClick = (e: Event) => {
      const target = e.target as HTMLElement;
      const zoomBtn = target.closest<HTMLButtonElement>('[data-action="zoom-mermaid"]');
      
      if (zoomBtn) {
        e.preventDefault();
        const targetId = zoomBtn.dataset.target;
        if (!targetId) return;
        
        const diagram = container.querySelector<HTMLElement>(`#${targetId}`);
        if (!diagram) return;
        
        // Get the SVG content
        const svg = diagram.querySelector('svg');
        if (!svg) return;
        
        // Create modal
        const modal = document.createElement('div');
        modal.className = 'md-mermaid-modal';
        modal.innerHTML = `
          <div class="md-mermaid-modal-content">
            <button class="md-mermaid-modal-close">
              <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M18 6L6 18M6 6l12 12"/>
              </svg>
            </button>
            ${svg.outerHTML}
          </div>
        `;
        
        // Close handlers
        const closeModal = () => modal.remove();
        modal.addEventListener('click', (e) => {
          if (e.target === modal) closeModal();
        });
        modal.querySelector('.md-mermaid-modal-close')?.addEventListener('click', closeModal);
        
        // Escape key
        const handleEscape = (e: KeyboardEvent) => {
          if (e.key === 'Escape') {
            closeModal();
            document.removeEventListener('keydown', handleEscape);
          }
        };
        document.addEventListener('keydown', handleEscape);
        
        document.body.appendChild(modal);
      }
    };
    
    container.addEventListener('click', handleClick);
    return () => container.removeEventListener('click', handleClick);
  }, [containerRef]);
}

/**
 * Standalone function to render all Mermaid diagrams in a container
 * Useful for non-React contexts
 */
export async function renderMermaidDiagrams(container: HTMLElement): Promise<void> {
  initMermaid();
  
  const diagrams = container.querySelectorAll<HTMLElement>(
    '.md-mermaid:not(.rendered):not(.error)'
  );
  
  for (const diagram of diagrams) {
    await renderDiagram(diagram);
  }
}
