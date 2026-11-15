import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { Separator } from '@/components/ui/separator';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Checkbox } from '@/components/ui/checkbox';
import { Switch } from '@/components/ui/switch';
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group';
import { Skeleton } from '@/components/ui/skeleton';
import { Label } from '@/components/ui/label';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog';
import { cn } from '@/lib/utils';

/** One named colour token, shown as a swatch plus the job it is allowed to do. */
function Swatch({ token, role }: { token: string; role: string }) {
  return (
    <div className="flex items-center gap-2.5">
      <div
        className="size-9 shrink-0 rounded-md ring-1 ring-border-strong ring-inset"
        style={{ background: `var(--${token})` }}
      />
      <div className="min-w-0">
        <div className="truncate font-mono text-[11px] text-foreground">{token}</div>
        <div className="truncate text-[11px] text-muted-foreground">{role}</div>
      </div>
    </div>
  );
}

function Section({ title, hint, children }: { title: string; hint?: string; children: React.ReactNode }) {
  return (
    <section className="space-y-3">
      <div className="flex items-baseline justify-between gap-3">
        <h2 className="text-[11px] font-semibold tracking-[0.08em] text-muted-foreground uppercase">
          {title}
        </h2>
        {hint && <span className="font-mono text-[10px] text-muted-foreground/70">{hint}</span>}
      </div>
      {children}
    </section>
  );
}

const TOKENS: Array<[string, string]> = [
  ['background', '页面底色'],
  ['panel', '面板'],
  ['raised', '浮起面'],
  ['card', '卡片'],
  ['border', '发丝边'],
  ['border-strong', '强分隔'],
  ['foreground', '主文字'],
  ['muted-foreground', '次文字'],
  ['primary', '主操作与激活'],
  ['secondary', '次要控件'],
  ['accent', '悬停底'],
  ['destructive', '错误'],
  ['warning', '待办'],
  ['success', '通过'],
  ['ring', '焦点环'],
  ['input', '输入框边'],
];

const TYPE_SCALE: Array<[string, string, string]> = [
  ['text-2xl', '24 / 32', '页面标题'],
  ['text-lg', '18 / 28', '分区标题'],
  ['text-sm', '14 / 22', '正文'],
  ['text-xs', '12 / 20', '控件与列表'],
  ['text-[11px]', '11 / 18', '标签与元信息'],
  ['font-mono text-[11px]', '11 / 18', '代码与路径'],
];

function Column({ theme }: { theme: 'light' | 'dark' }) {
  return (
    <div
      data-theme={theme}
      className="rounded-xl border border-border bg-background text-foreground"
    >
      <div className="flex items-center justify-between border-b border-border px-5 py-3">
        <div className="flex items-center gap-2">
          <span className="text-sm font-semibold">{theme === 'dark' ? '深色' : '浅色'}</span>
          <span className="font-mono text-[11px] text-muted-foreground">data-theme="{theme}"</span>
        </div>
        <Badge variant="outline" className="font-mono text-[10px]">
          {theme === 'dark' ? 'DEFAULT' : 'ALTERNATE'}
        </Badge>
      </div>

      <div className="space-y-7 p-5">
        <Section title="颜色" hint="16 tokens">
          <div className="grid grid-cols-2 gap-x-4 gap-y-3">
            {TOKENS.map(([token, role]) => (
              <Swatch key={token} token={token} role={role} />
            ))}
          </div>
        </Section>

        <Separator />

        <Section title="字阶" hint="4 档 + mono">
          <div className="space-y-2">
            {TYPE_SCALE.map(([cls, spec, note]) => (
              <div key={cls} className="flex items-baseline justify-between gap-4">
                <span className={cn(cls, 'truncate')}>模型导航 SysML Studio</span>
                <span className="shrink-0 font-mono text-[10px] text-muted-foreground">{spec}</span>
                <span className="w-20 shrink-0 text-right text-[11px] text-muted-foreground">{note}</span>
              </div>
            ))}
          </div>
        </Section>

        <Separator />

        <Section title="间距与圆角" hint="4 基准">
          <div className="flex items-end gap-2">
            {[4, 8, 12, 16, 24, 32].map((n) => (
              <div key={n} className="flex flex-col items-center gap-1.5">
                <div className="w-6 rounded-sm bg-primary/70" style={{ height: n }} />
                <span className="font-mono text-[10px] text-muted-foreground">{n}</span>
              </div>
            ))}
          </div>
          <div className="mt-3 flex items-center gap-2">
            {['rounded-sm', 'rounded-md', 'rounded-lg', 'rounded-xl'].map((r) => (
              <div
                key={r}
                className={cn('flex h-10 w-16 items-center justify-center border border-border bg-raised', r)}
              >
                <span className="font-mono text-[10px] text-muted-foreground">{r.replace('rounded-', '')}</span>
              </div>
            ))}
          </div>
        </Section>

        <Separator />

        <Section title="按钮" hint="6 变体 × 3 尺寸">
          <div className="flex flex-wrap gap-2">
            <Button size="sm">新建元素</Button>
            <Button size="sm" variant="secondary">次要</Button>
            <Button size="sm" variant="outline">描边</Button>
            <Button size="sm" variant="ghost">幽灵</Button>
            <Button size="sm" variant="destructive">删除</Button>
            <Button size="sm" variant="link">链接</Button>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Button size="xs">xs</Button>
            <Button size="sm">sm</Button>
            <Button size="default">default</Button>
            <Button size="icon" variant="outline" aria-label="设置">
              <span className="text-xs">⚙</span>
            </Button>
          </div>
        </Section>

        <Separator />

        <Section title="输入与选择">
          <div className="grid grid-cols-2 gap-2">
            <Input placeholder="筛选模型元素" className="h-8" />
            <Select defaultValue="gv">
              <SelectTrigger className="h-8 w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="gv">GV 通用视图</SelectItem>
                <SelectItem value="iv">IV 互连视图</SelectItem>
                <SelectItem value="tv">TV 追溯视图</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className="flex flex-wrap items-center gap-4">
            <div className="flex items-center gap-2">
              <Checkbox id={`c-${theme}`} defaultChecked />
              <Label htmlFor={`c-${theme}`} className="text-xs">显示构造型</Label>
            </div>
            <div className="flex items-center gap-2">
              <Switch id={`s-${theme}`} defaultChecked />
              <Label htmlFor={`s-${theme}`} className="text-xs">自动校验</Label>
            </div>
            <ToggleGroup type="single" defaultValue="extended" size="sm">
              <ToggleGroupItem value="extended">extended</ToggleGroupItem>
              <ToggleGroupItem value="compat">compat</ToggleGroupItem>
            </ToggleGroup>
          </div>
        </Section>

        <Separator />

        <Section title="状态与徽章">
          <div className="flex flex-wrap gap-2">
            <Badge>默认</Badge>
            <Badge variant="secondary">次要</Badge>
            <Badge variant="outline">描边</Badge>
            <Badge variant="destructive">3 error</Badge>
          </div>
          <div className="flex flex-wrap items-center gap-3 text-[11px]">
            <span className="inline-flex items-center gap-1.5 text-destructive">
              <span className="size-1.5 rounded-full bg-destructive" />E021 连接端点不存在
            </span>
            <span className="inline-flex items-center gap-1.5 text-warning">
              <span className="size-1.5 rounded-full bg-warning" />W012 无台账记录
            </span>
            <span className="inline-flex items-center gap-1.5 text-success">
              <span className="size-1.5 rounded-full bg-success" />56/56 通过
            </span>
          </div>
        </Section>

        <Separator />

        <Section title="浮层">
          <div className="flex flex-wrap items-center gap-2">
            <TooltipProvider>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button size="sm" variant="outline">悬停提示</Button>
                </TooltipTrigger>
                <TooltipContent>缩放适配整个模型</TooltipContent>
              </Tooltip>
            </TooltipProvider>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button size="sm" variant="outline">下拉菜单</Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="start">
                <DropdownMenuLabel>显示</DropdownMenuLabel>
                <DropdownMenuSeparator />
                <DropdownMenuItem>构造型</DropdownMenuItem>
                <DropdownMenuItem>成员</DropdownMenuItem>
                <DropdownMenuItem>端口</DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
            <Dialog>
              <DialogTrigger asChild>
                <Button size="sm" variant="outline">对话框</Button>
              </DialogTrigger>
              <DialogContent>
                <DialogHeader>
                  <DialogTitle>记录一条来源</DialogTitle>
                  <DialogDescription>
                    这个数字在台账里没有记录。写下它从哪来，门禁才会放行。
                  </DialogDescription>
                </DialogHeader>
                <Input placeholder="例如 GB/T 3811 第 5.2 条" />
              </DialogContent>
            </Dialog>
          </div>
        </Section>

        <Separator />

        <Section title="列表密度" hint="row 26px">
          <div className="overflow-hidden rounded-md border border-border">
            {[
              ['VehicleArchitecture', 'part def', true],
              ['Structure', 'package', false],
              ['Vehicle', 'part def', false],
              ['Powertrain', 'part def', false],
            ].map(([name, kind, active]) => (
              <div
                key={name as string}
                className={cn(
                  'flex h-[26px] items-center justify-between px-2.5 text-xs',
                  active ? 'bg-accent text-accent-foreground' : 'text-muted-foreground',
                )}
              >
                <span className="truncate">{name as string}</span>
                <span className="shrink-0 font-mono text-[10px] opacity-60">{kind as string}</span>
              </div>
            ))}
          </div>
        </Section>

        <Separator />

        <Section title="加载与空态">
          <div className="flex items-center gap-3">
            <Skeleton className="h-8 w-24" />
            <Skeleton className="h-8 flex-1" />
          </div>
          <div className="flex h-16 items-center justify-center rounded-md border border-dashed border-border text-[11px] text-muted-foreground">
            这个视图里没有元素
          </div>
        </Section>

        <Separator />

        <Section title="标签页">
          <Tabs defaultValue="problems">
            <TabsList>
              <TabsTrigger value="problems">问题 3</TabsTrigger>
              <TabsTrigger value="review">待评审 2</TabsTrigger>
              <TabsTrigger value="source">源码</TabsTrigger>
            </TabsList>
            <TabsContent value="problems" className="pt-3 text-xs text-muted-foreground">
              点击问题可定位到画布元素或源码行。
            </TabsContent>
            <TabsContent value="review" className="pt-3 text-xs text-muted-foreground">
              没有待评审的判定。
            </TabsContent>
            <TabsContent value="source" className="pt-3 text-xs text-muted-foreground">
              VehicleArchitecture.sysml
            </TabsContent>
          </Tabs>
        </Section>
      </div>
    </div>
  );
}

/** The design system's own kitchen sink: tokens and components, both palettes, one page. */
export function DesignKit() {
  return (
    <div className="min-h-screen bg-background p-6 text-foreground">
      <header className="mb-5 flex items-end justify-between gap-6">
        <div>
          <h1 className="text-lg font-semibold">SysML Studio 设计系统</h1>
          <p className="mt-1 text-xs text-muted-foreground">
            Token 与组件自检页。改一处 token，两侧同时变化。组件源码在本仓库内，可直接改。
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-3 font-mono text-[10px] text-muted-foreground">
          <span>tailwind 4</span>
          <span>shadcn new-york</span>
          <span>oklch</span>
          <span>radius 0.375rem</span>
        </div>
      </header>
      <div className="grid grid-cols-2 items-start gap-5">
        <Column theme="light" />
        <Column theme="dark" />
      </div>
    </div>
  );
}
