/**
 * Font utilities for file previews
 */

export const getFontFamilyCSS = (val: string): string => {
  switch (val) {
    case 'jetbrains':
      return '"JetBrains Mono", monospace';
    case 'meslo':
      return '"MesloLGM Nerd Font", monospace';
    case 'menlo':
      return 'Menlo, Monaco, "Courier New", monospace';
    case 'courier':
      return '"Courier New", Courier, monospace';
    default:
      return '"JetBrains Mono", monospace';
  }
};
