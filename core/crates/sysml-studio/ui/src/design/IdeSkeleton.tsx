import { useCallback, useState } from 'react';
import {
  Background,
  BackgroundVariant,
  Handle,
  Position,
  ReactFlow,
  type Edge,
  type Node,
  type NodeProps,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import {
  AlertTriangle,
  Box,
  ChevronDown,
  Download,
  FileCode2,
  Maximize2,
  Moon,
  Redo2,
  Search,
  Settings2,
  Sun,
  Undo2,
} from 'lucide-react';
import { cn } from '@/lib/utils';

/**
 * What the studio would look like with shadcn's visual language applied to an IDE shell.
 *
 * Nothing here is the real app: the point is to answer one question before rebuilding anything —
 * does this style survive contact with the three parts shadcn does not supply (a diagram canvas,
 * a code surface, a docking shell)? The canvas is React Flow with custom nodes, the code surface is
 * a static mock, and the shell is a plain CSS grid, so the answer is about tokens rather than
 * about whether some library happens to ship the right skin.
 */

type SpecKind = 'part def' | 'package' | 'requirement';

const MEMBERS: Record<string, string[]> = {
  Drone: ['takeoffMass : Real', 'endurance : Real'],
  Airframe: ['mass : Real = 1450'],
  Battery: ['energy : Real', 'mass : Real'],
  HoverEndurance: ['endurance >= 25 min'],
};

function SpecNode({ data, selected }: NodeProps) {
  const label = data.label as string;
  const kind = data.kind as SpecKind;
  const unmet = data.unmet as boolean | undefined;
  const members = MEMBERS[label] ?? [];
  return (
    <div
      className={cn(
        'relative w-[196px] rounded-md border bg-card',
        selected ? 'border-primary ring-1 ring-primary/40' : 'border-border',
        unmet && 'border-dashed border-warning/70',
      )}
    >
      {/* React Flow drops an edge whose endpoints have no handle, and it does so silently. The
          handles are invisible because SysML ports are drawn on the shape, not as dots. */}
      <Handle type="target" position={Position.Top} style={{ opacity: 0 }} />
      <Handle type="source" position={Position.Bottom} style={{ opacity: 0 }} />
      <div className="flex h-7 items-center justify-between gap-2 border-b border-border px-2.5">
        <span className="min-w-0 flex-1 truncate text-xs font-medium">{label}</span>
        <span className="shrink-0 font-mono text-[9px] text-muted-foreground">{kind}</span>
      </div>
      <div className="space-y-0.5 px-2.5 py-1.5">
        {members.map((m) => (
          <div key={m} className="truncate font-mono text-[10px] text-muted-foreground">
            {m}
          </div>
        ))}
      </div>
    </div>
  );
}

const nodeTypes = { spec: SpecNode };

const NODES: Node[] = [
  { id: 'Drone', type: 'spec', position: { x: 140, y: 20 }, data: { label: 'Drone', kind: 'part def' }, selected: true },
  { id: 'Airframe', type: 'spec', position: { x: 30, y: 160 }, data: { label: 'Airframe', kind: 'part def' } },
  { id: 'Battery', type: 'spec', position: { x: 250, y: 160 }, data: { label: 'Battery', kind: 'part def' } },
  {
    id: 'HoverEndurance',
    type: 'spec',
    position: { x: 140, y: 300 },
    data: { label: 'HoverEndurance', kind: 'requirement', unmet: true },
  },
];

const EDGES: Edge[] = [
  {
    id: 'e1',
    source: 'Drone',
    target: 'Airframe',
    type: 'smoothstep',
    style: { stroke: 'var(--border-strong)', strokeWidth: 1.2 },
  },
  {
    id: 'e2',
    source: 'Drone',
    target: 'Battery',
    type: 'smoothstep',
    style: { stroke: 'var(--border-strong)', strokeWidth: 1.2 },
  },
  {
    id: 'e3',
    source: 'Battery',
    target: 'HoverEndurance',
    type: 'smoothstep',
    style: { stroke: 'var(--warning)', strokeWidth: 1.2, strokeDasharray: '4 3' },
  },
];

const TREE = [
  { name: 'VehicleArchitecture', depth: 0, kind: 'package' },
  { name: 'Structure', depth: 1, kind: 'package' },
  { name: 'Drone', depth: 2, kind: 'part def', active: true },
  { name: 'Airframe', depth: 2, kind: 'part def' },
  { name: 'Battery', depth: 2, kind: 'part def' },
  { name: 'Requirements', depth: 1, kind: 'package' },
  { name: 'HoverEndurance', depth: 2, kind: 'requirement' },
];

const CODE = [
  ['1', 'package VehicleArchitecture {'],
  ['2', '  package Structure {'],
  ['3', '    part def Drone {'],
  ['4', '      attribute takeoffMass : Real;'],
  ['5', '      attribute endurance : Real;'],
  ['6', '    }'],
  ['7', '    part def Airframe {'],
  ['8', '      attribute mass : Real = 1450;'],
  ['9', '    }'],
  ['10', '  }'],
  ['11', '}'],
];

function IconButton({ children, label }: { children: React.ReactNode; label: string }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      className="inline-flex size-6 items-center justify-center rounded-sm text-muted-foreground hover:bg-accent hover:text-accent-foreground"
    >
      {children}
    </button>
  );
}

export function IdeSkeleton() {
  const [theme, setTheme] = useState<'dark' | 'light'>(
    (document.documentElement.dataset.theme as 'dark' | 'light') ?? 'dark',
  );

  const flip = useCallback(() => {
    const next = theme === 'dark' ? 'light' : 'dark';
    document.documentElement.dataset.theme = next;
    setTheme(next);
  }, [theme]);

  return (
    <div className="flex h-screen flex-col bg-background text-foreground">
      {/* top bar */}
      <header className="flex h-10 shrink-0 items-center gap-2.5 border-b border-border px-3">
        <div className="flex size-5 items-center justify-center rounded-sm bg-primary">
          <Box className="size-3 text-primary-foreground" />
        </div>
        <span className="text-xs font-semibold">SysML Studio</span>
        <span className="h-4 w-px bg-border" />
        <span className="font-mono text-[11px] text-muted-foreground">drone.sysml</span>
        <span className="inline-flex h-5 items-center gap-1.5 rounded-sm bg-success/10 px-1.5 text-[10px] text-success">
          <span className="size-1 rounded-full bg-success" />
          已保存
        </span>
        <div className="flex-1" />
        <div className="flex h-7 w-60 items-center gap-1.5 rounded-md border border-border bg-card px-2">
          <Search className="size-3 text-muted-foreground" />
          <span className="text-[11px] text-muted-foreground">搜索命令或元素</span>
          <div className="flex-1" />
          <kbd className="rounded-sm border border-border bg-muted px-1 font-mono text-[9px] text-muted-foreground">
            ⌘K
          </kbd>
        </div>
        <button
          type="button"
          onClick={flip}
          aria-label="切换主题"
          title="切换主题"
          className="inline-flex size-6 items-center justify-center rounded-sm text-muted-foreground hover:bg-accent hover:text-accent-foreground"
        >
          {theme === 'dark' ? <Sun className="size-3.5" /> : <Moon className="size-3.5" />}
        </button>
        <IconButton label="导出 SVG">
          <Download className="size-3.5" />
        </IconButton>
        <IconButton label="设置">
          <Settings2 className="size-3.5" />
        </IconButton>
      </header>

      <div className="flex min-h-0 flex-1">
        {/* navigator */}
        <aside className="flex w-[212px] shrink-0 flex-col border-r border-border">
          <div className="flex h-8 items-center justify-between px-3">
            <span className="text-[10px] font-semibold tracking-[0.08em] text-muted-foreground uppercase">
              结构
            </span>
            <ChevronDown className="size-3 text-muted-foreground" />
          </div>
          <div className="flex-1 overflow-hidden py-0.5">
            {TREE.map((node) => (
              <button
                key={node.name}
                type="button"
                className={cn(
                  'flex h-[26px] w-full items-center gap-1.5 pr-2 text-left text-xs',
                  node.active
                    ? 'bg-accent text-accent-foreground'
                    : 'text-muted-foreground hover:bg-accent/50',
                )}
                style={{ paddingLeft: 10 + node.depth * 12 }}
              >
                {node.kind === 'package' ? (
                  <Box className="size-3 shrink-0 opacity-60" />
                ) : (
                  <FileCode2 className="size-3 shrink-0 opacity-60" />
                )}
                <span className="truncate">{node.name}</span>
              </button>
            ))}
          </div>
          <div className="border-t border-border px-3 py-2 text-[10px] text-muted-foreground">
            拖动节点移动，滚轮缩放
          </div>
        </aside>

        {/* canvas + code + problems */}
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="flex min-h-0 flex-1">
            <div className="relative min-w-0 flex-1">
              <ReactFlow
                nodes={NODES}
                edges={EDGES}
                nodeTypes={nodeTypes}
                fitView
                fitViewOptions={{ padding: 0.22 }}
                proOptions={{ hideAttribution: true }}
                nodesDraggable={false}
                panOnScroll
              >
                <Background variant={BackgroundVariant.Dots} gap={18} size={1} color="var(--border)" />
              </ReactFlow>
              <div className="absolute top-2.5 left-2.5 flex h-7 items-center gap-1 rounded-md border border-border bg-popover px-1 shadow-sm">
                <span className="px-1.5 text-[10px] text-muted-foreground">视图</span>
                <span className="text-[11px] font-medium">通用视图</span>
                <span className="h-3 w-px bg-border" />
                <IconButton label="适配">
                  <Maximize2 className="size-3" />
                </IconButton>
                <IconButton label="撤销">
                  <Undo2 className="size-3" />
                </IconButton>
                <IconButton label="重做">
                  <Redo2 className="size-3" />
                </IconButton>
              </div>
            </div>

            {/* code surface */}
            <div className="flex w-[352px] shrink-0 flex-col border-l border-border">
              <div className="flex h-8 items-center gap-2 border-b border-border px-2.5">
                <span className="inline-flex h-5 items-center rounded-sm bg-accent px-1.5 font-mono text-[10px] text-accent-foreground">
                  drone.sysml
                </span>
                <div className="flex-1" />
                <span className="text-[10px] text-warning">问题 1</span>
              </div>
              <div className="flex-1 overflow-hidden py-1.5">
                {CODE.map(([n, line]) => (
                  <div
                    key={n}
                    className={cn('flex h-[19px] items-center gap-2 pr-2', n === '3' && 'bg-accent/60')}
                  >
                    <span className="w-7 shrink-0 text-right font-mono text-[10px] text-muted-foreground/60">
                      {n}
                    </span>
                    <span className="truncate font-mono text-[11px] text-muted-foreground">{line}</span>
                  </div>
                ))}
              </div>
            </div>
          </div>

          {/* problems */}
          <div className="flex h-[164px] shrink-0 flex-col border-t border-border">
            <div className="flex h-8 items-center gap-1 border-b border-border px-2">
              <span className="inline-flex h-6 items-center gap-1.5 rounded-sm bg-accent px-2 text-[11px] font-medium">
                问题 <span className="font-mono text-[10px]">1</span>
              </span>
              <span className="inline-flex h-6 items-center px-2 text-[11px] text-muted-foreground">
                待评审 <span className="ml-1 font-mono text-[10px]">2</span>
              </span>
              <span className="inline-flex h-6 items-center px-2 text-[11px] text-muted-foreground">源码</span>
              <div className="flex-1" />
              <IconButton label="收起">
                <ChevronDown className="size-3" />
              </IconButton>
            </div>
            <div className="flex-1 overflow-hidden">
              <div className="flex h-[26px] items-center gap-2 bg-accent/40 px-3">
                <AlertTriangle className="size-3 shrink-0 text-warning" />
                <span className="font-mono text-[10px] text-warning">W012</span>
                <span className="truncate text-[11px]">这个值没有台账记录</span>
                <div className="flex-1" />
                <span className="shrink-0 font-mono text-[10px] text-muted-foreground">Drone::takeoffMass</span>
              </div>
              <div className="flex h-[26px] items-center gap-2 px-3">
                <AlertTriangle className="size-3 shrink-0 text-destructive" />
                <span className="font-mono text-[10px] text-destructive">E034</span>
                <span className="truncate text-[11px] text-muted-foreground">没有任何结构承担这条需求</span>
                <div className="flex-1" />
                <span className="shrink-0 font-mono text-[10px] text-muted-foreground">HoverEndurance</span>
              </div>
            </div>
          </div>
        </div>

        {/* inspector */}
        <aside className="flex w-[248px] shrink-0 flex-col border-l border-border">
          <div className="flex h-8 items-center justify-between border-b border-border px-3">
            <span className="text-[10px] font-semibold tracking-[0.08em] text-muted-foreground uppercase">
              检查器
            </span>
            <Settings2 className="size-3 text-muted-foreground" />
          </div>
          <div className="space-y-3 p-3">
            <div>
              <div className="text-sm font-semibold">Drone</div>
              <div className="mt-1 flex items-center gap-1.5">
                <span className="rounded-sm bg-muted px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground">
                  part def
                </span>
                <span className="rounded-sm bg-primary/15 px-1.5 py-0.5 text-[10px] text-primary">已选中</span>
              </div>
            </div>
            <div className="grid grid-cols-3 gap-1.5">
              {[
                ['42', '元素'],
                ['68', '关系'],
                ['1', '问题'],
              ].map(([v, k]) => (
                <div key={k} className="rounded-md border border-border bg-card px-2 py-1.5">
                  <div className="text-sm font-semibold">{v}</div>
                  <div className="text-[10px] text-muted-foreground">{k}</div>
                </div>
              ))}
            </div>
            <div className="space-y-1.5">
              {[
                ['当前值', '—'],
                ['依据', '未记账'],
                ['反证', '未填写'],
              ].map(([k, v]) => (
                <div key={k} className="flex items-center justify-between text-[11px]">
                  <span className="text-muted-foreground">{k}</span>
                  <span className={v === '未记账' ? 'text-warning' : 'text-muted-foreground'}>{v}</span>
                </div>
              ))}
            </div>
            <div className="h-px bg-border" />
            <div className="space-y-1">
              <div className="text-[10px] font-semibold tracking-[0.08em] text-muted-foreground uppercase">
                成员
              </div>
              {MEMBERS.Drone.map((m) => (
                <div key={m} className="truncate font-mono text-[10px] text-muted-foreground">
                  {m}
                </div>
              ))}
            </div>
            <button
              type="button"
              className="h-7 w-full rounded-md bg-primary text-[11px] font-medium text-primary-foreground"
            >
              记录一条来源
            </button>
          </div>
        </aside>
      </div>

      {/* status bar */}
      <footer className="flex h-6 shrink-0 items-center gap-3 border-t border-border px-3 text-[10px] text-muted-foreground">
        <span className="inline-flex items-center gap-1.5">
          <span className="size-1.5 rounded-full bg-success" />
          src/model/drone.sysml
        </span>
        <div className="flex-1" />
        <span>42 元素</span>
        <span>68 关系</span>
        <span className="text-warning">1 warning</span>
        <span className="h-3 w-px bg-border" />
        <span className="font-mono">100%</span>
        <span className="font-mono">UTF-8</span>
      </footer>
    </div>
  );
}
