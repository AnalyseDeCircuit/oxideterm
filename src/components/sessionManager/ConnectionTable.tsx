// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useTranslation } from 'react-i18next';
import { ArrowUpDown, ArrowUp, ArrowDown, Server, Plus } from 'lucide-react';
import { cn } from '../../lib/utils';
import { Checkbox } from '../ui/checkbox';
import { Button } from '../ui/button';
import { ConnectionTableRow } from './ConnectionTableRow';
import { useAppStore } from '../../store/appStore';
import type { ConnectionInfo } from '../../types';
import type { SortField, SortDirection } from './useSessionManager';

type ConnectionTableProps = {
  connections: ConnectionInfo[];
  selectedIds: Set<string>;
  sortField: SortField | null;
  sortDirection: SortDirection;
  onToggleSort: (field: SortField) => void;
  onToggleSelect: (id: string) => void;
  onToggleSelectAll: () => void;
  onConnect: (id: string) => void;
  onEdit: (id: string) => void;
  onDuplicate: (conn: ConnectionInfo) => void;
  onDelete: (conn: ConnectionInfo) => void;
};

const SortIcon = ({ field, currentField, direction }: { field: SortField; currentField: SortField | null; direction: SortDirection }) => {
  if (field !== currentField) return <ArrowUpDown className="h-3.5 w-3.5 text-theme-text-muted opacity-40" />;
  return direction === 'asc'
    ? <ArrowUp className="h-3.5 w-3.5 text-blue-400" />
    : <ArrowDown className="h-3.5 w-3.5 text-blue-400" />;
};

export const ConnectionTable = ({
  connections,
  selectedIds,
  sortField,
  sortDirection,
  onToggleSort,
  onToggleSelect,
  onToggleSelectAll,
  onConnect,
  onEdit,
  onDuplicate,
  onDelete,
}: ConnectionTableProps) => {
  const { t } = useTranslation();
  const toggleModal = useAppStore(s => s.toggleModal);

  const isAllSelected = connections.length > 0 && selectedIds.size === connections.length;

  const columns: { key: SortField | 'actions' | 'checkbox'; label: string; sortable: boolean; className: string }[] = [
    { key: 'checkbox', label: '', sortable: false, className: 'w-8 shrink-0' },
    { key: 'name', label: t('sessionManager.table.name'), sortable: true, className: 'w-[140px] min-w-[100px] flex-1 pl-1' },
    { key: 'host', label: t('sessionManager.table.host'), sortable: true, className: 'w-[130px] shrink-0' },
    { key: 'port', label: t('sessionManager.table.port'), sortable: true, className: 'w-[50px] shrink-0' },
    { key: 'username', label: t('sessionManager.table.username'), sortable: true, className: 'w-[90px] shrink-0' },
    { key: 'auth_type', label: t('sessionManager.table.auth_type'), sortable: true, className: 'w-[72px] shrink-0' },
    { key: 'group', label: t('sessionManager.table.group'), sortable: true, className: 'w-[100px] shrink-0' },
    { key: 'last_used_at', label: t('sessionManager.table.last_used'), sortable: true, className: 'w-[90px] shrink-0' },
    { key: 'actions', label: '', sortable: false, className: 'w-[84px] shrink-0 sticky right-0 bg-theme-bg-secondary z-20' },
  ];

  if (connections.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-theme-text-muted gap-4 py-16">
        <Server className="h-12 w-12 opacity-30" />
        <div className="text-center">
          <p className="text-sm font-medium">{t('sessionManager.table.no_connections')}</p>
          <p className="text-xs mt-1">{t('sessionManager.table.no_connections_hint')}</p>
        </div>
        <Button size="sm" onClick={() => toggleModal('newConnection', true)} className="gap-1.5">
          <Plus className="h-4 w-4" />
          {t('sessionManager.toolbar.new_connection')}
        </Button>
      </div>
    );
  }

  return (
    <div className="h-full overflow-auto">
      <div className="min-w-fit flex flex-col">
        {/* Table header */}
        <div className="flex items-center border-b border-theme-border bg-theme-bg-secondary px-2 py-1.5 text-xs font-medium text-theme-text-muted sticky top-0 z-10">
          {columns.map(col => {
            if (col.key === 'checkbox') {
              return (
                <div key="checkbox" className={col.className + ' flex items-center justify-center'}>
                  <Checkbox
                    checked={isAllSelected}
                    onCheckedChange={onToggleSelectAll}
                  />
                </div>
              );
            }
            if (col.key === 'actions') {
              return <div key="actions" className={col.className} />;
            }
            return (
              <button
                key={col.key}
                className={cn(
                  col.className,
                  'flex items-center gap-1 text-left hover:text-theme-text transition-colors',
                  col.sortable && 'cursor-pointer'
                )}
                onClick={() => col.sortable && onToggleSort(col.key as SortField)}
              >
                {col.label}
                {col.sortable && (
                  <SortIcon field={col.key as SortField} currentField={sortField} direction={sortDirection} />
                )}
              </button>
            );
          })}
        </div>

        {/* Table body */}
        {connections.map(conn => (
          <ConnectionTableRow
            key={conn.id}
            connection={conn}
            isSelected={selectedIds.has(conn.id)}
            onToggleSelect={onToggleSelect}
            onConnect={onConnect}
            onEdit={onEdit}
            onDuplicate={onDuplicate}
            onDelete={onDelete}
          />
        ))}
      </div>
    </div>
  );
};
