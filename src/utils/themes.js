// id, label, mode ('light'|'dark'), swatch [bg, primary, success]
export const THEMES = [
  { id: 'system', label: 'System', mode: 'auto', swatch: ['#ffffff', '#1a1a1a', '#0066cc'] },
  { id: 'light', label: 'Light', mode: 'light', swatch: ['#ffffff', '#0066cc', '#28a745'] },
  { id: 'dark', label: 'Dark', mode: 'dark', swatch: ['#1a1a1a', '#4d9fff', '#40d160'] },
  { id: 'breeze-light', label: 'Breeze Light', mode: 'light', swatch: ['#eff0f1', '#3daee9', '#27ae60'] },
  { id: 'breeze-dark', label: 'Breeze Dark', mode: 'dark', swatch: ['#232629', '#3daee9', '#27ae60'] },
  { id: 'catppuccin-latte', label: 'Catppuccin Latte', mode: 'light', swatch: ['#eff1f5', '#1e66f5', '#40a02b'] },
  { id: 'catppuccin-frappe', label: 'Catppuccin Frappé', mode: 'dark', swatch: ['#303446', '#8caaee', '#a6d189'] },
  {
    id: 'catppuccin-macchiato',
    label: 'Catppuccin Macchiato',
    mode: 'dark',
    swatch: ['#24273a', '#8aadf4', '#a6da95'],
  },
  { id: 'catppuccin-mocha', label: 'Catppuccin Mocha', mode: 'dark', swatch: ['#1e1e2e', '#89b4fa', '#a6e3a1'] },
  { id: 'nord', label: 'Nord', mode: 'dark', swatch: ['#2e3440', '#88c0d0', '#a3be8c'] },
  { id: 'gruvbox-dark', label: 'Gruvbox Dark', mode: 'dark', swatch: ['#282828', '#83a598', '#b8bb26'] },
  { id: 'gruvbox-light', label: 'Gruvbox Light', mode: 'light', swatch: ['#fbf1c7', '#076678', '#79740e'] },
  { id: 'tokyo-night', label: 'Tokyo Night', mode: 'dark', swatch: ['#1a1b26', '#7aa2f7', '#9ece6a'] },
  { id: 'rose-pine', label: 'Rosé Pine', mode: 'dark', swatch: ['#191724', '#c4a7e7', '#9ccfd8'] },
  { id: 'rose-pine-dawn', label: 'Rosé Pine Dawn', mode: 'light', swatch: ['#faf4ed', '#907aa9', '#56949f'] },
];

export const THEME_IDS = new Set(THEMES.map((t) => t.id));
export const MODE_BY_ID = Object.fromEntries(THEMES.map((t) => [t.id, t.mode]));
