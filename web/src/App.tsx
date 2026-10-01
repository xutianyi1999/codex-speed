import {
  ArrowDownLeftIcon,
  ArrowUpRightIcon,
  CheckIcon,
  CopyIcon,
  GaugeIcon,
  InfoIcon,
  LayersIcon,
  RadioIcon,
  Settings2Icon,
  TimerIcon,
  WifiOffIcon,
} from "lucide-react";
import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Table,
  TableBody,
  TableCaption,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { clock, rate, type Snapshot, seconds, tokens, useMetrics } from "@/lib/api";
import { copyText } from "@/lib/clipboard";

const Trend = lazy(() => import("@/components/trend"));

const windows = [
  { value: 15, label: "15 分钟" },
  { value: 60, label: "1 小时" },
  { value: 1440, label: "24 小时" },
  { value: 10080, label: "7 天" },
];
function Hint({ children, label }: { children: React.ReactNode; label: string }) {
  return (
    <Tooltip>
      <TooltipTrigger render={<Button variant="ghost" size="icon-xs" aria-label={label} />}>
        <InfoIcon />
      </TooltipTrigger>
      <TooltipContent>{children}</TooltipContent>
    </Tooltip>
  );
}
function Setup({ endpoint, children }: { endpoint: string; children?: React.ReactNode }) {
  const [copied, setCopied] = useState(false);
  const [copyError, setCopyError] = useState(false);
  const commandRef = useRef<HTMLElement>(null);
  const command = `OTEL_METRIC_EXPORT_INTERVAL=1000 codex --enable runtime_metrics \\\n  -c 'otel.metrics_exporter={otlp-http={endpoint="${endpoint}",protocol="json"}}'`;
  return (
    <Dialog>
      <DialogTrigger render={<Button variant={children ? "default" : "secondary"} size="lg" />}>
        <Settings2Icon data-icon="inline-start" />
        {children || "连接 Codex"}
      </DialogTrigger>
      <DialogContent className="setup-dialog">
        <DialogHeader>
          <DialogTitle>连接你的 Codex</DialogTitle>
          <DialogDescription>
            保持 codex-speed 运行，在另一个终端用下面的命令启动 Codex。
          </DialogDescription>
        </DialogHeader>
        <div className="command-block">
          <pre>
            <code ref={commandRef}>{command}</code>
          </pre>
          <Button
            variant="secondary"
            size="sm"
            onClick={async (event) => {
              const container = event.currentTarget.parentElement;
              const success = container != null && (await copyText(command, container));
              setCopied(success);
              setCopyError(!success);
              if (!success && commandRef.current) {
                window.getSelection()?.selectAllChildren(commandRef.current);
              }
            }}
          >
            {copied ? (
              <CheckIcon data-icon="inline-start" />
            ) : (
              <CopyIcon data-icon="inline-start" />
            )}
            {copied ? "已复制" : "复制命令"}
          </Button>
        </div>
        {copyError && <p role="alert">已选中命令，请按 Ctrl+C（macOS 用 ⌘C）复制。</p>}
        <ol className="setup-steps">
          <li>先启动监控，再启动 Codex；已运行的 Codex 需要重新启动。</li>
          <li>正常提问或写代码，收到原生 metrics 后页面会自动更新。</li>
          <li>首 token 与 TBT 通常在服务端返回计时后出现，token 用量在 turn 结束后更新。</li>
        </ol>
        <p className="text-sm text-muted-foreground">
          设置只对这次启动生效。服务端计时需要提供方支持，缺失时显示 —。
        </p>
      </DialogContent>
    </Dialog>
  );
}
function Details({ data }: { data: Snapshot }) {
  const summary = data.summary;
  const names: Record<string, string> = {
    engine: "Engine 耗时",
    overhead: "服务端额外耗时",
    iapi_ttft: "IAPI TTFT",
    iapi_tbt: "IAPI TBT",
  };
  return (
    <Dialog>
      <DialogTrigger render={<Button variant="ghost" size="sm" />}>
        <InfoIcon data-icon="inline-start" />
        指标说明
      </DialogTrigger>
      <DialogContent className="details-dialog">
        <DialogHeader>
          <DialogTitle>指标口径与详情</DialogTitle>
          <DialogDescription>
            采用服务端原生计时，不使用整轮耗时或客户端网络计时。
          </DialogDescription>
        </DialogHeader>
        <dl className="definitions">
          <div>
            <dt>首 token 延迟</dt>
            <dd>服务端报告的 Service TTFT，主数值为所选窗口内的样本平均值。</dd>
          </div>
          <div>
            <dt>估算 Decode 吞吐</dt>
            <dd>1000 ÷ 平均 Service TBT（毫秒）。这是服务端 TBT 的倒数估算，不是逐 token 测量。</dd>
          </div>
          <div>
            <dt>Token 用量</dt>
            <dd>
              原生 turn.token_usage
              的报告总量。输入包含缓存输入，两者不相加。不同字段分别统计，缺失不补零。
            </dd>
          </div>
          <div>
            <dt>P50 / P95</dt>
            <dd>
              从直方图估算；单样本批次可使用精确值。P95 至少需要 20 个样本。采集批次不等于请求或
              turn。
            </dd>
          </div>
        </dl>
        <p className="text-sm text-muted-foreground">
          TTFT P50 {seconds(summary.ttft.p50_ms)} s · P95 {seconds(summary.ttft.p95_ms)} s
          {summary.ttft.quantiles_approximate ? "（直方图估算）" : ""}
        </p>
        <Table>
          <TableCaption className="sr-only">所选窗口的服务端计时详情</TableCaption>
          <TableHeader>
            <TableRow>
              <TableHead scope="col">指标</TableHead>
              <TableHead scope="col">平均</TableHead>
              <TableHead scope="col">样本</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {[
              ["Service TBT", summary.tbt],
              ...Object.entries(summary.details).map(
                ([key, value]) => [names[key] || key, value] as const,
              ),
            ].map(([name, value]) => (
              <TableRow key={String(name)}>
                <TableCell>{String(name)}</TableCell>
                <TableCell>
                  {typeof value !== "string" && value.mean_ms != null
                    ? `${value.mean_ms.toFixed(2)} ms`
                    : "—"}
                </TableCell>
                <TableCell>{typeof value !== "string" ? value.samples : "—"}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </DialogContent>
    </Dialog>
  );
}
export default function App() {
  const [minutes, setMinutes] = useState(60);
  const [model, setModel] = useState<string | null>(null);
  const [now, setNow] = useState(Date.now());
  const { data, error, connected } = useMetrics(minutes, model);
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, []);
  useEffect(() => {
    if (model == null && data?.selected_model) setModel(data.selected_model);
  }, [model, data?.selected_model]);
  const selectedModel = model ?? data?.selected_model ?? null;
  const summary = data?.summary;
  const endpoint = data?.endpoint || `${window.location.origin}/v1/metrics`;
  const last = data?.last_received_ms;
  const fresh = last != null && now - last < 10_000;
  const status = !connected
    ? "连接中"
    : last == null
      ? "等待 Codex"
      : fresh
        ? "正在接收"
        : "等待新数据";
  const modelItems = data?.models.map((m) => ({ value: m.model, label: m.model })) || [];
  if (selectedModel != null && !modelItems.some((item) => item.value === selectedModel))
    modelItems.push({ value: selectedModel, label: selectedModel });
  return (
    <div className="app-shell">
      <a className="skip-link" href="#content">
        跳到监控内容
      </a>
      <header className="app-header">
        <a href="/" className="brand">
          <img className="brand-mark" src="/favicon.svg" alt="" width="38" height="38" />
          <span>
            Codex <strong>Speed</strong>
          </span>
        </a>
        <div className="header-actions">
          <Badge variant="secondary">
            {connected ? (
              <RadioIcon data-icon="inline-start" />
            ) : (
              <WifiOffIcon data-icon="inline-start" />
            )}
            {status}
          </Badge>
          <Setup endpoint={endpoint} />
        </div>
      </header>
      <main id="content" tabIndex={-1}>
        <section className="intro">
          <div className="eyebrow">
            <span className="status-dot" />
            模型服务监控
          </div>
          <h1>服务表现，一目了然。</h1>
          <p>观察首 token 等待、生成速度和 token 用量。</p>
        </section>
        {error && (
          <Alert variant="destructive">
            <WifiOffIcon />
            <AlertTitle>暂时无法连接监控</AlertTitle>
            <AlertDescription>{error.message} 页面会自动重试。</AlertDescription>
          </Alert>
        )}
        <div className="toolbar">
          <FieldGroup className="model-field">
            <Field orientation="horizontal">
              <FieldLabel htmlFor="model-select">模型</FieldLabel>
              <Select
                items={modelItems}
                value={selectedModel}
                disabled={modelItems.length === 0}
                onValueChange={(v) => v && setModel(v)}
              >
                <SelectTrigger id="model-select">
                  <SelectValue placeholder="等待模型数据" />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    {modelItems.map((item) => (
                      <SelectItem key={item.value} value={item.value}>
                        {item.label}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
            </Field>
          </FieldGroup>
          <ToggleGroup
            aria-label="统计时间范围"
            value={[String(minutes)]}
            onValueChange={(v) => v[0] && setMinutes(Number(v[0]))}
            spacing={1}
          >
            {windows.map((w) => (
              <ToggleGroupItem key={w.value} value={String(w.value)}>
                {w.label}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
        </div>
        <section aria-label="主要指标" className="metrics-grid">
          <Card tone="blue">
            <CardHeader>
              <CardTitle>首 token 延迟</CardTitle>
              <CardDescription>服务端 TTFT · 平均</CardDescription>
              <CardAction>
                <Hint label="首 token 延迟说明">
                  服务端报告的 Service TTFT，不含客户端网络延迟。
                </Hint>
              </CardAction>
            </CardHeader>
            <CardContent>
              <div className="metric-value">
                {seconds(summary?.ttft.mean_ms)}
                <span>s</span>
              </div>
              <div className="metric-context">
                <TimerIcon aria-hidden="true" />
                开始生成前的等待
              </div>
            </CardContent>
            <CardFooter>
              <span>{summary?.ttft.samples || 0} 个有效样本</span>
              <span>
                P50 {seconds(summary?.ttft.p50_ms)} s
                {summary?.ttft.quantiles_approximate && summary.ttft.p50_ms != null ? " ≈" : ""}
              </span>
            </CardFooter>
          </Card>
          <Card tone="green">
            <CardHeader>
              <CardTitle>
                Decode 吞吐 <Badge variant="secondary">估算</Badge>
              </CardTitle>
              <CardDescription>基于平均 Service TBT</CardDescription>
              <CardAction>
                <Hint label="Decode 吞吐说明">
                  1000 ÷ 平均 Service TBT（ms），反映所选窗口的服务端计时，不是逐 token 实测。
                </Hint>
              </CardAction>
            </CardHeader>
            <CardContent>
              <div className="metric-value">
                {rate(summary?.decode_tps)}
                <span>tok/s</span>
              </div>
              <div className="metric-context">
                <GaugeIcon aria-hidden="true" />首 token 之后的生成速度参考
              </div>
            </CardContent>
            <CardFooter>
              <span>{summary?.tbt.samples || 0} 个有效样本</span>
              <span>
                TBT {summary?.tbt.mean_ms == null ? "—" : summary.tbt.mean_ms.toFixed(1)} ms
              </span>
            </CardFooter>
          </Card>
          <Card>
            <CardHeader>
              <CardTitle>Token 用量</CardTitle>
              <CardDescription>所选窗口内的报告总量</CardDescription>
              <CardAction>
                <Hint label="Token 用量说明">
                  输入包含缓存输入。各字段分别统计，缺失显示 —，不补零。
                </Hint>
              </CardAction>
            </CardHeader>
            <CardContent>
              <div className="metric-value">
                {tokens(summary?.output_tokens)}
                <span>tokens</span>
              </div>
              <div className="metric-context">
                <ArrowUpRightIcon aria-hidden="true" />
                输出 tokens
              </div>
            </CardContent>
            <CardFooter>
              <span>输入 {tokens(summary?.input_tokens)}</span>
              <span>缓存 {tokens(summary?.cached_input_tokens)}</span>
            </CardFooter>
          </Card>
        </section>
        <section className="trend-section">
          <div className="section-heading">
            <div>
              <h2>近期趋势</h2>
              <p>按时间分桶的平均值，空白表示没有数据。</p>
            </div>
            {data && <Details data={data} />}
          </div>
          <div className="charts-grid">
            <div className="chart-panel">
              <div className="chart-heading">
                <span className="legend-dot" />
                <h3>首 token 延迟</h3>
                <span>s · 越低越快</span>
              </div>
              {data ? (
                <Suspense fallback={<div className="chart-empty" />}>
                  <Trend data={data} metric="ttft_ms" />
                </Suspense>
              ) : (
                <div className="chart-empty" />
              )}
            </div>
            <div className="chart-panel">
              <div className="chart-heading" data-tone="green">
                <span className="legend-dot" />
                <h3>估算 Decode 吞吐</h3>
                <span>tok/s · 越高越快</span>
              </div>
              {data ? (
                <Suspense fallback={<div className="chart-empty" />}>
                  <Trend data={data} metric="decode_tps" />
                </Suspense>
              ) : (
                <div className="chart-empty" />
              )}
            </div>
          </div>
        </section>
        <section className="models-section">
          <div className="section-heading">
            <div>
              <h2>模型表现</h2>
              <p>同一时间窗口内，分别观察每个模型。</p>
            </div>
            <Badge variant="secondary">{data?.models.length || 0} 个模型</Badge>
          </div>
          {data?.models.length ? (
            <Table>
              <TableCaption className="sr-only">各模型的服务端计时和 token 用量</TableCaption>
              <TableHeader>
                <TableRow>
                  <TableHead scope="col">模型</TableHead>
                  <TableHead scope="col">平均 TTFT</TableHead>
                  <TableHead scope="col">估算 Decode</TableHead>
                  <TableHead scope="col">输入 / 缓存</TableHead>
                  <TableHead scope="col">输出</TableHead>
                  <TableHead scope="col">TTFT / TBT 样本</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {data.models.map((m) => (
                  <TableRow key={m.model}>
                    <TableCell>
                      <Button variant="ghost" onClick={() => setModel(m.model)}>
                        <LayersIcon data-icon="inline-start" />
                        {m.model}
                      </Button>
                    </TableCell>
                    <TableCell>{seconds(m.ttft.mean_ms)} s</TableCell>
                    <TableCell>{rate(m.decode_tps)} tok/s</TableCell>
                    <TableCell>
                      {tokens(m.input_tokens)} / {tokens(m.cached_input_tokens)}
                    </TableCell>
                    <TableCell>{tokens(m.output_tokens)} tokens</TableCell>
                    <TableCell>
                      {m.ttft.samples} / {m.tbt.samples}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          ) : (
            <Empty>
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <ArrowDownLeftIcon />
                </EmptyMedia>
                <EmptyTitle>
                  {last == null ? "等你的第一个样本" : "这个窗口内还没有有效样本"}
                </EmptyTitle>
                <EmptyDescription>
                  {last == null
                    ? "连接 Codex 后，正常使用即可开始监控。"
                    : "已收到遥测。计时可能尚未返回，或当前服务不提供这些指标；也可以扩大时间范围。"}
                </EmptyDescription>
              </EmptyHeader>
              <EmptyContent>
                <Setup endpoint={endpoint}>开始采集</Setup>
              </EmptyContent>
            </Empty>
          )}
        </section>
        <footer className="page-footer">
          <span>
            <RadioIcon aria-hidden="true" />
            {last == null ? "尚未收到遥测" : `最后接收 ${clock(last)}`}
          </span>
          <span>本地采集 · 保留 7 天 · 服务端原生 metrics</span>
        </footer>
      </main>
    </div>
  );
}
