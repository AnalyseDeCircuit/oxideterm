/**
 * Plugin UI Kit
 *
 * A lightweight component library exposed to plugins via window.__OXIDE__.ui.
 * These components wrap OxideTerm's theme system into simple, createElement-friendly
 * primitives that plugins can use without knowing Tailwind class names.
 *
 * All components are pure React function components — no Radix UI, no cva, no forwardRef.
 * Plugins call them via: h(ui.Card, { title: '...' }, children)
 *
 * Theme integration:
 *   - Uses OxideTerm's CSS variable-based theme classes (bg-theme-*, text-theme-*, etc.)
 *   - Automatically adapts to all built-in and custom themes
 */

import React from 'react';

// ─── Utility ────────────────────────────────────────────────────────────

function cx(...classes: (string | false | null | undefined)[]): string {
  return classes.filter(Boolean).join(' ');
}

// ═══════════════════════════════════════════════════════════════════════════
// Layout Components
// ═══════════════════════════════════════════════════════════════════════════

// ─── ScrollView ─────────────────────────────────────────────────────────

export type ScrollViewProps = {
  className?: string;
  maxWidth?: string;
  padding?: string;
  children?: React.ReactNode;
};

/**
 * Full-height scrollable container. Ideal as Tab root.
 *
 * ```js
 * h(ui.ScrollView, null,
 *   h(ui.Card, { title: 'Hello' }, 'content'),
 * )
 * ```
 */
const maxWidthMap: Record<string, string> = {
  sm: '24rem', md: '28rem', lg: '32rem', xl: '36rem',
  '2xl': '42rem', '3xl': '48rem', '4xl': '56rem',
  '5xl': '64rem', '6xl': '72rem', '7xl': '80rem',
  full: '100%', none: 'none',
};

const spacingMap: Record<string | number, string> = {
  0: '0px', 1: '0.25rem', 2: '0.5rem', 3: '0.75rem',
  4: '1rem', 5: '1.25rem', 6: '1.5rem', 8: '2rem',
  10: '2.5rem', 12: '3rem', 16: '4rem',
};

export function ScrollView({ className, maxWidth = '4xl', padding = '6', children }: ScrollViewProps) {
  const mw = maxWidthMap[maxWidth] ?? maxWidth;
  const pad = spacingMap[padding] ?? padding;
  return (
    <div className={cx('h-full overflow-auto', className)} style={{ padding: pad }}>
      <div className="space-y-6 mx-auto" style={{ maxWidth: mw }}>
        {children}
      </div>
    </div>
  );
}

// ─── Stack ──────────────────────────────────────────────────────────────

export type StackProps = {
  direction?: 'vertical' | 'horizontal';
  gap?: number;
  align?: 'start' | 'center' | 'end' | 'stretch' | 'baseline';
  justify?: 'start' | 'center' | 'end' | 'between' | 'around';
  wrap?: boolean;
  className?: string;
  children?: React.ReactNode;
};

/**
 * Flexbox stack layout.
 *
 * ```js
 * h(ui.Stack, { direction: 'horizontal', gap: 2 },
 *   h(ui.Button, null, 'A'),
 *   h(ui.Button, null, 'B'),
 * )
 * ```
 */
export function Stack({
  direction = 'vertical',
  gap = 2,
  align,
  justify,
  wrap,
  className,
  children,
}: StackProps) {
  const alignMap = { start: 'items-start', center: 'items-center', end: 'items-end', stretch: 'items-stretch', baseline: 'items-baseline' };
  const justifyMap = { start: 'justify-start', center: 'justify-center', end: 'justify-end', between: 'justify-between', around: 'justify-around' };
  return (
    <div
      className={cx(
        'flex',
        direction === 'horizontal' ? 'flex-row' : 'flex-col',
        align && alignMap[align],
        justify && justifyMap[justify],
        wrap && 'flex-wrap',
        className,
      )}
      style={{ gap: spacingMap[gap] ?? `${gap * 0.25}rem` }}
    >
      {children}
    </div>
  );
}

// ─── Grid ───────────────────────────────────────────────────────────────

export type GridProps = {
  cols?: number;
  gap?: number;
  className?: string;
  children?: React.ReactNode;
};

/**
 * CSS Grid layout.
 *
 * ```js
 * h(ui.Grid, { cols: 3, gap: 4 }, ...cards)
 * ```
 */
export function Grid({ cols = 2, gap = 4, className, children }: GridProps) {
  return (
    <div
      className={cx('grid', className)}
      style={{
        gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))`,
        gap: spacingMap[gap] ?? `${gap * 0.25}rem`,
      }}
    >
      {children}
    </div>
  );
}

// ═══════════════════════════════════════════════════════════════════════════
// Container Components
// ═══════════════════════════════════════════════════════════════════════════

// ─── Card ───────────────────────────────────────────────────────────────

export type CardProps = {
  title?: string;
  icon?: React.ComponentType<{ className?: string }>;
  className?: string;
  headerRight?: React.ReactNode;
  children?: React.ReactNode;
};

/**
 * Themed card with optional icon + title header.
 *
 * ```js
 * h(ui.Card, { icon: Activity, title: 'Metrics' },
 *   h('p', null, 'Card content'),
 * )
 * ```
 */
export function Card({ title, icon: Icon, className, headerRight, children }: CardProps) {
  return (
    <div className={cx('border border-theme-border rounded-sm p-4 bg-theme-bg-panel', className)}>
      {(title || Icon) && (
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-2">
            {Icon && <Icon className="h-4 w-4 text-theme-accent" />}
            {title && <h3 className="text-sm font-semibold text-theme-text">{title}</h3>}
          </div>
          {headerRight}
        </div>
      )}
      {children}
    </div>
  );
}

// ─── Stat ───────────────────────────────────────────────────────────────

export type StatProps = {
  label: string;
  value: string | number;
  icon?: React.ComponentType<{ className?: string }>;
  className?: string;
};

/**
 * Single stat display card (icon + number + label).
 *
 * ```js
 * h(ui.Stat, { icon: Activity, label: 'Output', value: '12.5 KB' })
 * ```
 */
export function Stat({ label, value, icon: Icon, className }: StatProps) {
  return (
    <div className={cx('border border-theme-border rounded-sm p-3 bg-theme-bg-panel text-center', className)}>
      {Icon && <Icon className="h-4 w-4 text-theme-accent mx-auto mb-1" />}
      <div className="text-lg font-bold text-theme-text">{String(value)}</div>
      <div className="text-xs text-theme-text-muted">{label}</div>
    </div>
  );
}

// ═══════════════════════════════════════════════════════════════════════════
// Form Components
// ═══════════════════════════════════════════════════════════════════════════

// ─── Button ─────────────────────────────────────────────────────────────

export type ButtonProps = {
  variant?: 'primary' | 'secondary' | 'destructive' | 'ghost' | 'outline';
  size?: 'sm' | 'md' | 'lg' | 'icon';
  type?: 'button' | 'submit' | 'reset';
  disabled?: boolean;
  onClick?: (e: React.MouseEvent<HTMLButtonElement>) => void;
  className?: string;
  children?: React.ReactNode;
};

const buttonVariantClasses: Record<string, string> = {
  primary: 'bg-theme-text text-theme-bg hover:opacity-90 border border-transparent',
  secondary: 'bg-theme-bg-panel text-theme-text border border-theme-border hover:bg-theme-bg-hover',
  destructive: 'bg-red-900 text-red-100 hover:bg-red-800 border border-red-900',
  ghost: 'text-theme-text hover:bg-theme-bg-hover bg-transparent border border-transparent',
  outline: 'border border-theme-border bg-transparent hover:bg-theme-bg-hover text-theme-text',
};

const buttonSizeClasses: Record<string, string> = {
  sm: 'h-7 px-2 text-xs',
  md: 'h-8 px-3 text-sm',
  lg: 'h-9 px-4 text-sm',
  icon: 'h-8 w-8 p-0',
};

/**
 * Themed button with variants.
 *
 * ```js
 * h(ui.Button, { variant: 'primary', onClick: handler }, 'Click Me')
 * h(ui.Button, { variant: 'ghost', size: 'sm' }, h(RefreshCw, { className: 'h-3 w-3' }))
 * ```
 */
export function Button({
  variant = 'secondary',
  size = 'md',
  type = 'button',
  disabled,
  onClick,
  className,
  children,
}: ButtonProps) {
  return (
    <button
      type={type}
      onClick={onClick}
      disabled={disabled}
      className={cx(
        'inline-flex items-center justify-center gap-1.5 rounded-sm font-medium transition-colors',
        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-theme-accent',
        'disabled:pointer-events-none disabled:opacity-50',
        buttonVariantClasses[variant],
        buttonSizeClasses[size],
        className,
      )}
    >
      {children}
    </button>
  );
}

// ─── Input ──────────────────────────────────────────────────────────────

export type InputProps = {
  value?: string;
  defaultValue?: string;
  placeholder?: string;
  type?: string;
  disabled?: boolean;
  onChange?: (e: React.ChangeEvent<HTMLInputElement>) => void;
  onKeyDown?: (e: React.KeyboardEvent<HTMLInputElement>) => void;
  className?: string;
  size?: 'sm' | 'md';
};

/**
 * Themed text input.
 *
 * ```js
 * h(ui.Input, { value, onChange: (e) => setValue(e.target.value), placeholder: 'Search...' })
 * ```
 */
export function Input({
  value,
  defaultValue,
  placeholder,
  type = 'text',
  disabled,
  onChange,
  onKeyDown,
  className,
  size = 'md',
}: InputProps) {
  const sizeClass = size === 'sm' ? 'h-7 text-xs' : 'h-8 text-sm';
  return (
    <input
      type={type}
      value={value}
      defaultValue={defaultValue}
      placeholder={placeholder}
      disabled={disabled}
      onChange={onChange}
      onKeyDown={onKeyDown}
      className={cx(
        'w-full rounded-sm border border-theme-border bg-theme-bg-panel px-2 text-theme-text',
        'placeholder:text-theme-text-muted transition-colors',
        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-theme-accent',
        'disabled:cursor-not-allowed disabled:opacity-50',
        sizeClass,
        className,
      )}
    />
  );
}

// ─── Checkbox ───────────────────────────────────────────────────────────

export type CheckboxProps = {
  checked?: boolean;
  onChange?: (checked: boolean) => void;
  label?: string;
  disabled?: boolean;
  className?: string;
};

/**
 * Simple checkbox with optional label.
 *
 * ```js
 * h(ui.Checkbox, { checked: val, onChange: setVal, label: 'Enable feature' })
 * ```
 */
export function Checkbox({ checked, onChange, label, disabled, className }: CheckboxProps) {
  return (
    <label className={cx('inline-flex items-center gap-2 cursor-pointer', disabled && 'opacity-50 cursor-not-allowed', className)}>
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(e) => onChange?.(e.target.checked)}
        className="h-4 w-4 rounded-sm border border-theme-border bg-theme-bg-panel accent-[var(--accent)] cursor-pointer disabled:cursor-not-allowed"
      />
      {label && <span className="text-sm text-theme-text select-none">{label}</span>}
    </label>
  );
}

// ─── Select ─────────────────────────────────────────────────────────────

export type SelectOption = {
  label: string;
  value: string | number;
};

export type SelectProps = {
  value?: string | number;
  options: SelectOption[];
  onChange?: (value: string | number) => void;
  disabled?: boolean;
  placeholder?: string;
  className?: string;
  size?: 'sm' | 'md';
};

/**
 * Native select dropdown.
 *
 * ```js
 * h(ui.Select, {
 *   value: theme,
 *   options: [{ label: 'Dark', value: 'dark' }, { label: 'Light', value: 'light' }],
 *   onChange: setTheme,
 * })
 * ```
 */
export function Select({ value, options, onChange, disabled, placeholder, className, size = 'md' }: SelectProps) {
  const sizeClass = size === 'sm' ? 'h-7 text-xs' : 'h-8 text-sm';
  const handleChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
    const raw = e.target.value;
    // Map back to the original option value type (preserve number if declared as number)
    const matched = options.find((opt) => String(opt.value) === raw);
    onChange?.(matched ? matched.value : raw);
  };
  return (
    <select
      value={value != null ? String(value) : undefined}
      disabled={disabled}
      onChange={handleChange}
      className={cx(
        'rounded-sm border border-theme-border bg-theme-bg-panel px-2 text-theme-text',
        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-theme-accent',
        'disabled:cursor-not-allowed disabled:opacity-50',
        sizeClass,
        className,
      )}
    >
      {placeholder && <option value="" disabled>{placeholder}</option>}
      {options.map((opt) => (
        <option key={String(opt.value)} value={String(opt.value)}>{opt.label}</option>
      ))}
    </select>
  );
}

// ═══════════════════════════════════════════════════════════════════════════
// Typography & Display
// ═══════════════════════════════════════════════════════════════════════════

// ─── Text ───────────────────────────────────────────────────────────────

export type TextProps = {
  variant?: 'heading' | 'subheading' | 'body' | 'muted' | 'mono' | 'label' | 'tiny';
  as?: 'p' | 'span' | 'h1' | 'h2' | 'h3' | 'div' | 'label';
  className?: string;
  children?: React.ReactNode;
};

const textVariantClasses: Record<string, string> = {
  heading: 'text-xl font-bold text-theme-text',
  subheading: 'text-sm font-semibold text-theme-text',
  body: 'text-sm text-theme-text',
  muted: 'text-xs text-theme-text-muted',
  mono: 'text-xs font-mono text-theme-text',
  label: 'text-xs font-semibold text-theme-text-muted uppercase tracking-wider',
  tiny: 'text-[10px] text-theme-text-muted',
};

/**
 * Themed text with semantic variants.
 *
 * ```js
 * h(ui.Text, { variant: 'heading' }, 'Dashboard')
 * h(ui.Text, { variant: 'muted' }, 'Last updated: 5 min ago')
 * h(ui.Text, { variant: 'mono' }, '192.168.1.1')
 * ```
 */
export function Text({ variant = 'body', as: Tag = 'div', className, children }: TextProps) {
  return <Tag className={cx(textVariantClasses[variant], className)}>{children}</Tag>;
}

// ─── Badge ──────────────────────────────────────────────────────────────

export type BadgeProps = {
  variant?: 'default' | 'success' | 'warning' | 'error' | 'info';
  className?: string;
  children?: React.ReactNode;
};

const badgeVariantClasses: Record<string, string> = {
  default: 'bg-theme-bg-hover text-theme-text border-theme-border',
  success: 'bg-green-900/50 text-green-300 border-green-700/50',
  warning: 'bg-yellow-900/50 text-yellow-300 border-yellow-700/50',
  error: 'bg-red-900/50 text-red-300 border-red-700/50',
  info: 'bg-blue-900/50 text-blue-300 border-blue-700/50',
};

/**
 * Inline status badge.
 *
 * ```js
 * h(ui.Badge, { variant: 'success' }, 'Active')
 * h(ui.Badge, { variant: 'error' }, 'Error')
 * ```
 */
export function Badge({ variant = 'default', className, children }: BadgeProps) {
  return (
    <span className={cx(
      'inline-flex items-center rounded-sm border px-1.5 py-0.5 text-[10px] font-semibold',
      badgeVariantClasses[variant],
      className,
    )}>
      {children}
    </span>
  );
}

// ─── Separator ──────────────────────────────────────────────────────────

export type SeparatorProps = {
  className?: string;
};

/**
 * Horizontal separator line.
 *
 * ```js
 * h(ui.Separator)
 * ```
 */
export function Separator({ className }: SeparatorProps) {
  return <div className={cx('h-px w-full bg-theme-border', className)} />;
}

// ─── IconText ───────────────────────────────────────────────────────────

export type IconTextProps = {
  icon: React.ComponentType<{ className?: string }>;
  children?: React.ReactNode;
  iconClass?: string;
  className?: string;
};

/**
 * Inline icon + text combo.
 *
 * ```js
 * h(ui.IconText, { icon: Terminal }, 'Active Sessions')
 * ```
 */
export function IconText({ icon: Icon, children, iconClass, className }: IconTextProps) {
  return (
    <div className={cx('flex items-center gap-2', className)}>
      <Icon className={cx('h-4 w-4 text-theme-accent', iconClass)} />
      <span className="text-sm text-theme-text">{children}</span>
    </div>
  );
}

// ─── KV (Key-Value row) ─────────────────────────────────────────────────

export type KVProps = {
  label: string;
  children?: React.ReactNode;
  mono?: boolean;
  className?: string;
};

/**
 * Key-value display row (label left, value right).
 *
 * ```js
 * h(ui.KV, { label: 'Host' }, '192.168.1.1')
 * h(ui.KV, { label: 'Status', mono: true }, 'active')
 * ```
 */
export function KV({ label, children, mono, className }: KVProps) {
  return (
    <div className={cx('flex items-center justify-between px-2 py-1.5 rounded-sm bg-theme-bg-panel border border-theme-border', className)}>
      <span className="text-xs text-theme-text-muted">{label}</span>
      <span className={cx('text-xs text-theme-text', mono && 'font-mono')}>{children}</span>
    </div>
  );
}

// ─── EmptyState ─────────────────────────────────────────────────────────

export type EmptyStateProps = {
  icon?: React.ComponentType<{ className?: string }>;
  title: string;
  description?: string;
  action?: React.ReactNode;
  className?: string;
};

/**
 * Empty state placeholder.
 *
 * ```js
 * h(ui.EmptyState, {
 *   icon: Inbox,
 *   title: 'No items',
 *   description: 'Start by adding a new item.',
 *   action: h(ui.Button, { variant: 'primary' }, 'Add Item'),
 * })
 * ```
 */
export function EmptyState({ icon: Icon, title, description, action, className }: EmptyStateProps) {
  return (
    <div className={cx('flex flex-col items-center justify-center py-8 text-center', className)}>
      {Icon && <Icon className="h-8 w-8 text-theme-text-muted mb-3" />}
      <div className="text-sm font-medium text-theme-text mb-1">{title}</div>
      {description && <div className="text-xs text-theme-text-muted mb-3 max-w-xs">{description}</div>}
      {action}
    </div>
  );
}

// ─── List / ListItem ────────────────────────────────────────────────────

export type ListItemProps = {
  icon?: React.ComponentType<{ className?: string }>;
  title: string;
  subtitle?: string;
  right?: React.ReactNode;
  onClick?: () => void;
  active?: boolean;
  className?: string;
};

/**
 * Clickable list item row.
 *
 * ```js
 * h(ui.ListItem, {
 *   icon: Server,
 *   title: 'production-01',
 *   subtitle: 'root@10.0.1.1',
 *   right: h(ui.Badge, { variant: 'success' }, 'Active'),
 *   onClick: () => { ... },
 * })
 * ```
 */
export function ListItem({ icon: Icon, title, subtitle, right, onClick, active, className }: ListItemProps) {
  const isInteractive = !!onClick;
  return (
    <div
      role={isInteractive ? 'button' : undefined}
      tabIndex={isInteractive ? 0 : undefined}
      onClick={onClick}
      onKeyDown={isInteractive ? (e: React.KeyboardEvent) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onClick?.();
        }
      } : undefined}
      className={cx(
        'flex items-center gap-3 px-3 py-2 rounded-sm transition-colors',
        isInteractive && 'cursor-pointer',
        isInteractive && 'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-theme-accent',
        active ? 'bg-theme-accent/10 border border-theme-accent/30' : 'hover:bg-theme-bg-hover border border-transparent',
        className,
      )}
    >
      {Icon && <Icon className="h-4 w-4 text-theme-accent shrink-0" />}
      <div className="flex-1 min-w-0">
        <div className="text-sm text-theme-text truncate">{title}</div>
        {subtitle && <div className="text-xs text-theme-text-muted truncate">{subtitle}</div>}
      </div>
      {right}
    </div>
  );
}

// ═══════════════════════════════════════════════════════════════════════════
// Composite / Higher-level
// ═══════════════════════════════════════════════════════════════════════════

// ─── Tabs ───────────────────────────────────────────────────────────────

export type TabItem = {
  id: string;
  label: string;
  icon?: React.ComponentType<{ className?: string }>;
};

export type TabsProps = {
  tabs: TabItem[];
  activeTab: string;
  onTabChange: (tabId: string) => void;
  className?: string;
  children?: React.ReactNode;
};

/**
 * Simple tab bar + content container. No Radix dependency.
 *
 * ```js
 * const [tab, setTab] = useState('overview');
 * h(ui.Tabs, {
 *   tabs: [
 *     { id: 'overview', label: 'Overview', icon: Activity },
 *     { id: 'logs', label: 'Logs', icon: FileText },
 *   ],
 *   activeTab: tab,
 *   onTabChange: setTab,
 * },
 *   tab === 'overview' ? h(OverviewPanel) : h(LogsPanel),
 * )
 * ```
 */
export function Tabs({ tabs: tabItems, activeTab, onTabChange, className, children }: TabsProps) {
  return (
    <div className={cx('flex flex-col', className)}>
      <div className="flex border-b border-theme-border mb-3">
        {tabItems.map((t) => (
          <button
            key={t.id}
            onClick={() => onTabChange(t.id)}
            className={cx(
              'flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium transition-colors -mb-px border-b-2',
              t.id === activeTab
                ? 'border-theme-accent text-theme-accent'
                : 'border-transparent text-theme-text-muted hover:text-theme-text hover:border-theme-border',
            )}
          >
            {t.icon && <t.icon className="h-3.5 w-3.5" />}
            {t.label}
          </button>
        ))}
      </div>
      <div className="flex-1 min-h-0">{children}</div>
    </div>
  );
}

// ─── Table ──────────────────────────────────────────────────────────────

export type TableColumn<T = Record<string, unknown>> = {
  key: string;
  header: string;
  width?: string;
  align?: 'left' | 'center' | 'right';
  render?: (value: unknown, row: T, index: number) => React.ReactNode;
};

export type TableProps<T = Record<string, unknown>> = {
  columns: TableColumn<T>[];
  data: T[];
  emptyText?: string;
  compact?: boolean;
  striped?: boolean;
  className?: string;
  onRowClick?: (row: T, index: number) => void;
};

/**
 * Data table with typed columns.
 *
 * ```js
 * h(ui.Table, {
 *   columns: [
 *     { key: 'host', header: 'Host' },
 *     { key: 'status', header: 'Status', render: (v) => h(ui.Badge, { variant: v === 'active' ? 'success' : 'error' }, v) },
 *   ],
 *   data: connections,
 *   onRowClick: (row) => selectConnection(row.id),
 * })
 * ```
 */
export function Table<T extends Record<string, unknown>>({
  columns,
  data,
  emptyText = 'No data',
  compact,
  striped,
  className,
  onRowClick,
}: TableProps<T>) {
  const cellPad = compact ? 'px-2 py-1' : 'px-3 py-2';
  const alignClass = (a?: string) => a === 'right' ? 'text-right' : a === 'center' ? 'text-center' : 'text-left';

  return (
    <div className={cx('border border-theme-border rounded-sm overflow-hidden', className)}>
      <div className="overflow-x-auto">
        <table className="w-full text-xs">
          <thead>
            <tr className="bg-theme-bg border-b border-theme-border">
              {columns.map((col) => (
                <th
                  key={col.key}
                  className={cx('font-semibold text-theme-text-muted', cellPad, alignClass(col.align))}
                  style={col.width ? { width: col.width } : undefined}
                >
                  {col.header}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {data.length === 0 ? (
              <tr>
                <td colSpan={columns.length} className={cx('text-center text-theme-text-muted', cellPad)}>
                  {emptyText}
                </td>
              </tr>
            ) : (
              data.map((row, i) => (
                <tr
                  key={i}
                  onClick={onRowClick ? () => onRowClick(row, i) : undefined}
                  className={cx(
                    'border-b border-theme-border last:border-b-0 transition-colors',
                    onRowClick && 'cursor-pointer hover:bg-theme-bg-hover',
                    striped && i % 2 === 1 && 'bg-theme-bg/50',
                  )}
                >
                  {columns.map((col) => (
                    <td key={col.key} className={cx('text-theme-text', cellPad, alignClass(col.align))}>
                      {col.render ? col.render(row[col.key], row, i) : String(row[col.key] ?? '')}
                    </td>
                  ))}
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}

// ─── Progress ───────────────────────────────────────────────────────────

export type ProgressProps = {
  value: number;
  max?: number;
  variant?: 'default' | 'success' | 'warning' | 'error';
  showLabel?: boolean;
  className?: string;
};

const progressColors: Record<string, string> = {
  default: 'bg-theme-accent',
  success: 'bg-green-500',
  warning: 'bg-yellow-500',
  error: 'bg-red-500',
};

/**
 * Progress bar.
 *
 * ```js
 * h(ui.Progress, { value: 75, variant: 'success', showLabel: true })
 * ```
 */
export function Progress({ value, max = 100, variant = 'default', showLabel, className }: ProgressProps) {
  const pct = Math.min(100, Math.max(0, (value / max) * 100));
  return (
    <div className={cx('flex items-center gap-2', className)}>
      <div className="flex-1 h-2 rounded-full bg-theme-bg-panel border border-theme-border overflow-hidden">
        <div
          className={cx('h-full rounded-full transition-all duration-300', progressColors[variant])}
          style={{ width: `${pct}%` }}
        />
      </div>
      {showLabel && <span className="text-[10px] text-theme-text-muted tabular-nums">{Math.round(pct)}%</span>}
    </div>
  );
}

// ─── Toggle ─────────────────────────────────────────────────────────────

export type ToggleProps = {
  checked?: boolean;
  onChange?: (checked: boolean) => void;
  label?: string;
  disabled?: boolean;
  className?: string;
};

/**
 * On/off toggle switch.
 *
 * ```js
 * h(ui.Toggle, { checked: enabled, onChange: setEnabled, label: 'Auto-refresh' })
 * ```
 */
export function Toggle({ checked, onChange, label, disabled, className }: ToggleProps) {
  return (
    <label className={cx('inline-flex items-center gap-2 cursor-pointer select-none', disabled && 'opacity-50 cursor-not-allowed', className)}>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        disabled={disabled}
        onClick={() => onChange?.(!checked)}
        className={cx(
          'relative inline-flex h-5 w-9 shrink-0 items-center rounded-full border-2 border-transparent transition-colors',
          'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-theme-accent',
          'disabled:cursor-not-allowed',
          checked ? 'bg-theme-accent' : 'bg-theme-bg-hover',
        )}
      >
        <span
          className={cx(
            'pointer-events-none block h-3.5 w-3.5 rounded-full bg-white shadow-sm transition-transform',
            checked ? 'translate-x-4' : 'translate-x-0.5',
          )}
        />
      </button>
      {label && <span className="text-sm text-theme-text">{label}</span>}
    </label>
  );
}

// ─── Alert ──────────────────────────────────────────────────────────────

export type AlertProps = {
  variant?: 'info' | 'success' | 'warning' | 'error';
  title?: string;
  icon?: React.ComponentType<{ className?: string }>;
  className?: string;
  children?: React.ReactNode;
};

const alertColors: Record<string, string> = {
  info: 'border-blue-700/50 bg-blue-900/20 text-blue-300',
  success: 'border-green-700/50 bg-green-900/20 text-green-300',
  warning: 'border-yellow-700/50 bg-yellow-900/20 text-yellow-300',
  error: 'border-red-700/50 bg-red-900/20 text-red-300',
};

/**
 * Alert / callout box.
 *
 * ```js
 * h(ui.Alert, { variant: 'warning', title: 'Caution' }, 'This action cannot be undone.')
 * h(ui.Alert, { variant: 'info', icon: Info }, 'Press Ctrl+D to trigger.')
 * ```
 */
export function Alert({ variant = 'info', title, icon: Icon, className, children }: AlertProps) {
  return (
    <div className={cx('rounded-sm border p-3', alertColors[variant], className)}>
      <div className="flex gap-2">
        {Icon && <Icon className="h-4 w-4 shrink-0 mt-0.5" />}
        <div className="flex-1 text-xs">
          {title && <div className="font-semibold mb-0.5">{title}</div>}
          <div className="opacity-90">{children}</div>
        </div>
      </div>
    </div>
  );
}

// ─── Spinner ────────────────────────────────────────────────────────────

export type SpinnerProps = {
  size?: 'sm' | 'md' | 'lg';
  label?: string;
  className?: string;
};

/**
 * Loading spinner.
 *
 * ```js
 * h(ui.Spinner, { size: 'sm', label: 'Loading...' })
 * ```
 */
export function Spinner({ size = 'md', label, className }: SpinnerProps) {
  const sizeMap = { sm: 'h-4 w-4', md: 'h-6 w-6', lg: 'h-8 w-8' };
  return (
    <div className={cx('flex items-center justify-center gap-2', className)}>
      <svg className={cx('animate-spin text-theme-accent', sizeMap[size])} xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
        <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
        <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
      </svg>
      {label && <span className="text-xs text-theme-text-muted">{label}</span>}
    </div>
  );
}

// ─── CodeBlock ──────────────────────────────────────────────────────────

export type CodeBlockProps = {
  children?: React.ReactNode;
  maxHeight?: string;
  wrap?: boolean;
  className?: string;
};

/**
 * Monospace code / terminal output block.
 *
 * ```js
 * h(ui.CodeBlock, { maxHeight: '200px' },
 *   'ssh root@192.168.1.1\nPassword:\nLast login: ...',
 * )
 * ```
 */
export function CodeBlock({ children, maxHeight = '300px', wrap, className }: CodeBlockProps) {
  return (
    <pre
      className={cx(
        'rounded-sm border border-theme-border bg-theme-bg p-3 text-xs font-mono text-theme-text overflow-auto',
        wrap && 'whitespace-pre-wrap break-words',
        className,
      )}
      style={{ maxHeight }}
    >
      <code>{children}</code>
    </pre>
  );
}

// ─── Header ─────────────────────────────────────────────────────────────

export type HeaderProps = {
  icon?: React.ComponentType<{ className?: string }>;
  title: string;
  subtitle?: string;
  action?: React.ReactNode;
  className?: string;
};

/**
 * Page/section header with icon, title, subtitle, and optional action.
 *
 * ```js
 * h(ui.Header, {
 *   icon: LayoutDashboard,
 *   title: 'Plugin Dashboard',
 *   subtitle: `v1.0.0 · ${lang}`,
 *   action: h(ui.Button, { size: 'sm' }, 'Refresh'),
 * })
 * ```
 */
export function Header({ icon: Icon, title, subtitle, action, className }: HeaderProps) {
  return (
    <div className={cx('flex items-center gap-3 mb-2', className)}>
      {Icon && <Icon className="h-6 w-6 text-theme-accent" />}
      <div className="flex-1">
        <h1 className="text-xl font-bold text-theme-text">{title}</h1>
        {subtitle && <p className="text-sm text-theme-text-muted">{subtitle}</p>}
      </div>
      {action}
    </div>
  );
}

// ═══════════════════════════════════════════════════════════════════════════
// Export all components as the UI kit
// ═══════════════════════════════════════════════════════════════════════════

export const pluginUIKit = {
  // Layout
  ScrollView,
  Stack,
  Grid,
  // Containers
  Card,
  Stat,
  // Form
  Button,
  Input,
  Checkbox,
  Select,
  Toggle,
  // Typography & Display
  Text,
  Badge,
  Separator,
  IconText,
  KV,
  EmptyState,
  ListItem,
  // Feedback
  Progress,
  Alert,
  Spinner,
  // Data
  Table,
  CodeBlock,
  // Composite
  Tabs,
  Header,
} as const;

export type PluginUIKit = typeof pluginUIKit;
