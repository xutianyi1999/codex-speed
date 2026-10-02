import {
  ArrowUpRightIcon,
  CheckIcon,
  CopyIcon,
  GaugeIcon,
  InfoIcon,
  LanguagesIcon,
  RadioIcon,
  Settings2Icon,
  TimerIcon,
  WifiOffIcon,
} from "lucide-react";
import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import RecentReports from "@/components/recent-reports";
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
import {
  clock,
  duration,
  percent,
  rate,
  type Snapshot,
  seconds,
  tokens,
  useMetrics,
} from "@/lib/api";
import { copyText } from "@/lib/clipboard";

const Trend = lazy(() => import("@/components/trend"));

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
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  const [copyError, setCopyError] = useState(false);
  const commandRef = useRef<HTMLElement>(null);
  const command = `OTEL_METRIC_EXPORT_INTERVAL=1000 codex --enable runtime_metrics \\\n  -c 'otel.metrics_exporter={otlp-http={endpoint="${endpoint}",protocol="json"}}'`;
  return (
    <Dialog>
      <DialogTrigger render={<Button variant={children ? "default" : "secondary"} size="lg" />}>
        <Settings2Icon data-icon="inline-start" />
        {children || t("connect")}
      </DialogTrigger>
      <DialogContent className="setup-dialog">
        <DialogHeader>
          <DialogTitle>{t("setup_title")}</DialogTitle>
          <DialogDescription>{t("setup_description")}</DialogDescription>
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
            {copied ? t("copied") : t("copy_command")}
          </Button>
        </div>
        {copyError && <p role="alert">{t("copy_manual")}</p>}
        <ol className="setup-steps">
          <li>{t("setup_step1")}</li>
          <li>{t("setup_step2")}</li>
          <li>{t("setup_step3")}</li>
        </ol>
        <p className="text-sm text-muted-foreground">{t("setup_note")}</p>
      </DialogContent>
    </Dialog>
  );
}
function Details({ data }: { data: Snapshot }) {
  const { t } = useTranslation();
  const summary = data.summary;
  const names: Record<string, string> = {
    engine: t("engine"),
    overhead: t("overhead"),
    iapi_ttft: "IAPI TTFT",
    iapi_tbt: "IAPI TBT",
  };
  return (
    <Dialog>
      <DialogTrigger render={<Button variant="ghost" size="sm" />}>
        <InfoIcon data-icon="inline-start" />
        {t("metric_details")}
      </DialogTrigger>
      <DialogContent className="details-dialog">
        <DialogHeader>
          <DialogTitle>{t("details_title")}</DialogTitle>
          <DialogDescription>{t("details_description")}</DialogDescription>
        </DialogHeader>
        <dl className="definitions">
          <div>
            <dt>{t("ttft")}</dt>
            <dd>{t("ttft_definition")}</dd>
          </div>
          <div>
            <dt>{t("decode_estimated")}</dt>
            <dd>{t("decode_definition")}</dd>
          </div>
          <div>
            <dt>{t("token_usage")}</dt>
            <dd>{t("token_definition")}</dd>
          </div>
          <div>
            <dt>{t("cache_share")}</dt>
            <dd>{t("cache_definition")}</dd>
          </div>
          <div>
            <dt>{t("reliability")}</dt>
            <dd>{t("reliability_definition")}</dd>
          </div>
          <div>
            <dt>P50 / P95</dt>
            <dd>{t("quantiles_definition")}</dd>
          </div>
        </dl>
        <p className="text-sm text-muted-foreground">
          TTFT P50 {duration(summary.ttft.p50_ms)} · P95 {duration(summary.ttft.p95_ms)}
          {summary.ttft.quantiles_approximate ? t("histogram_estimate") : ""}
        </p>
        <Table>
          <TableCaption className="sr-only">{t("details_caption")}</TableCaption>
          <TableHeader>
            <TableRow>
              <TableHead scope="col">{t("metric")}</TableHead>
              <TableHead scope="col">{t("window_mean")}</TableHead>
              <TableHead scope="col">{t("observations")}</TableHead>
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
                <TableCell>{typeof value !== "string" ? duration(value.mean_ms) : "—"}</TableCell>
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
  const { t, i18n } = useTranslation();
  const windows = [
    { value: 15, label: t("range_15") },
    { value: 60, label: t("range_60") },
    { value: 1440, label: t("range_1440") },
    { value: 10080, label: t("range_10080") },
  ];
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
    ? t("connecting")
    : last == null
      ? t("waiting_codex")
      : fresh
        ? t("receiving")
        : t("waiting_data");
  const modelItems = data?.models.map((m) => ({ value: m.model, label: m.model })) || [];
  if (selectedModel != null && !modelItems.some((item) => item.value === selectedModel))
    modelItems.push({ value: selectedModel, label: selectedModel });
  return (
    <div className="app-shell">
      <a className="skip-link" href="#content">
        {t("skip_content")}
      </a>
      <header className="app-header">
        <a href="/" className="brand">
          <img className="brand-mark" src="/favicon.svg" alt="" width="38" height="38" />
          <span>
            Codex <strong>Speed</strong>
          </span>
        </a>
        <div className="header-actions">
          <Select
            items={[
              { value: "zh", label: "简体中文" },
              { value: "en", label: "English" },
            ]}
            value={i18n.resolvedLanguage || "en"}
            onValueChange={(value) => {
              if (value) void i18n.changeLanguage(value);
            }}
          >
            <SelectTrigger className="language-select" aria-label="Language / 语言">
              <LanguagesIcon aria-hidden="true" />
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                <SelectItem value="zh">简体中文</SelectItem>
                <SelectItem value="en">English</SelectItem>
              </SelectGroup>
            </SelectContent>
          </Select>
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
            {t("monitor")}
          </div>
          <h1>{t("hero_title")}</h1>
          <p>{t("hero_description")}</p>
        </section>
        {error && (
          <Alert variant="destructive">
            <WifiOffIcon />
            <AlertTitle>{t("connection_error")}</AlertTitle>
            <AlertDescription>
              {t("fetch_error")}
              {t("retry_note")}
            </AlertDescription>
          </Alert>
        )}
        <div className="toolbar">
          <FieldGroup className="model-field">
            <Field orientation="horizontal">
              <FieldLabel htmlFor="model-select">{t("model")}</FieldLabel>
              <Select
                items={modelItems}
                value={selectedModel}
                disabled={modelItems.length === 0}
                onValueChange={(v) => v && setModel(v)}
              >
                <SelectTrigger id="model-select">
                  <SelectValue placeholder={t("waiting_model")} />
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
            aria-label={t("time_window")}
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
        <section aria-label={t("main_metrics")} className="metrics-grid">
          <Card tone="blue">
            <CardHeader>
              <CardTitle>{t("ttft")}</CardTitle>
              <CardDescription>{t("ttft_window")}</CardDescription>
              <CardAction>
                <Hint label={t("ttft_hint_title")}>{t("ttft_hint")}</Hint>
              </CardAction>
            </CardHeader>
            <CardContent>
              <div className="metric-value">
                {seconds(summary?.ttft.mean_ms)}
                <span>s</span>
              </div>
              <div className="metric-context">
                <TimerIcon aria-hidden="true" />
                {t("wait_before_generation")}
              </div>
              <div className="metric-secondary">
                <span className="metric-stat">
                  P50{" "}
                  <strong>
                    {seconds(summary?.ttft.p50_ms)} s
                    {summary?.ttft.quantiles_approximate && summary.ttft.p50_ms != null ? " ≈" : ""}
                  </strong>
                </span>
                <span className="metric-stat">
                  P95{" "}
                  <strong>
                    {seconds(summary?.ttft.p95_ms)} s
                    {summary?.ttft.quantiles_approximate && summary.ttft.p95_ms != null ? " ≈" : ""}
                  </strong>
                </span>
              </div>
            </CardContent>
            <CardFooter>
              <span>{t("observation_count", { count: summary?.ttft.samples || 0 })}</span>
            </CardFooter>
          </Card>
          <Card tone="green">
            <CardHeader>
              <CardTitle>
                {t("decode")} <Badge variant="secondary">{t("estimated")}</Badge>
              </CardTitle>
              <CardDescription>{t("decode_window")}</CardDescription>
              <CardAction>
                <Hint label={t("decode_hint_title")}>{t("decode_hint")}</Hint>
              </CardAction>
            </CardHeader>
            <CardContent>
              <div className="metric-value">
                {rate(summary?.decode_tps)}
                <span>tok/s</span>
              </div>
              <div className="metric-context">
                <GaugeIcon aria-hidden="true" />
                {t("decode_context")}
              </div>
              <div className="metric-secondary">
                <span className="metric-stat">
                  {t("mean_tbt")}{" "}
                  <strong>
                    {summary?.tbt.mean_ms == null ? "—" : summary.tbt.mean_ms.toFixed(1)} ms
                  </strong>
                </span>
              </div>
            </CardContent>
            <CardFooter>
              <span>{t("observation_count", { count: summary?.tbt.samples || 0 })}</span>
            </CardFooter>
          </Card>
          <Card>
            <CardHeader>
              <CardTitle>{t("token_usage")}</CardTitle>
              <CardDescription>{t("tokens_window")}</CardDescription>
              <CardAction>
                <Hint label={t("tokens_hint_title")}>{t("tokens_hint")}</Hint>
              </CardAction>
            </CardHeader>
            <CardContent>
              <div className="metric-value">
                {tokens(summary?.output_tokens)}
                <span>tokens</span>
              </div>
              <div className="metric-context">
                <ArrowUpRightIcon aria-hidden="true" />
                {t("output_total")}
              </div>
              <div className="metric-secondary">
                <span className="metric-stat">
                  {t("cache_share")} <strong>{percent(summary?.cached_input_percent)}</strong>
                </span>
                <span className="metric-stat">
                  {t("reasoning_output")}{" "}
                  <strong>{tokens(summary?.reasoning_output_tokens)}</strong>
                </span>
              </div>
            </CardContent>
            <CardFooter>
              <span>
                {t("input")} {tokens(summary?.input_tokens)}
              </span>
              <span>
                {t("cached_input")} {tokens(summary?.cached_input_tokens)}
              </span>
            </CardFooter>
          </Card>
        </section>
        <section className="trend-section">
          <div className="section-heading">
            <div>
              <h2>{t("recent_trends")}</h2>
              <p>{t("timing_trend_description")}</p>
            </div>
            {data && <Details data={data} />}
          </div>
          <div className="charts-grid">
            <div className="chart-panel">
              <div className="chart-heading">
                <span className="legend-dot" />
                <h3>{t("mean_ttft_label")}</h3>
                <span>{t("lower_faster")}</span>
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
                <h3>{t("decode_estimated")}</h3>
                <span>{t("higher_faster")}</span>
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
        <section className="trend-section" aria-label={t("token_trend")}>
          <div className="section-heading">
            <div>
              <h2>{t("token_trend")}</h2>
              <p>{t("token_trend_description")}</p>
            </div>
            <span className="text-xs text-muted-foreground">tokens</span>
          </div>
          {data ? (
            <Suspense fallback={<div className="chart-empty" />}>
              <Trend data={data} metric="tokens" />
            </Suspense>
          ) : (
            <div className="chart-empty" />
          )}
        </section>
        <section className="trend-section" aria-label={t("reliability")}>
          <div className="section-heading">
            <div>
              <h2>{t("reliability")}</h2>
              <p>{t("reliability_description")}</p>
            </div>
          </div>
          <div className="attempt-summary">
            <div>
              <span className="legend-dot" />
              <span>{t("http_failure")}</span>
              <strong>{percent(summary?.http_attempts?.failure_percent)}</strong>
              <span className="attempt-count">
                {summary?.http_attempts
                  ? t("attempt_count", {
                      failed: tokens(summary.http_attempts.failed),
                      total: tokens(summary.http_attempts.total),
                    })
                  : t("waiting_http")}
              </span>
            </div>
            <div>
              <span className="legend-dot" data-tone="purple" />
              <span>{t("ws_failure")}</span>
              <strong>{percent(summary?.websocket_send_attempts?.failure_percent)}</strong>
              <span className="attempt-count">
                {summary?.websocket_send_attempts
                  ? t("attempt_count", {
                      failed: tokens(summary.websocket_send_attempts.failed),
                      total: tokens(summary.websocket_send_attempts.total),
                    })
                  : t("waiting_ws")}
              </span>
            </div>
          </div>
          {data ? (
            <Suspense fallback={<div className="chart-empty" />}>
              <Trend data={data} metric="reliability" />
            </Suspense>
          ) : (
            <div className="chart-empty" />
          )}
        </section>
        {data && <RecentReports data={data} />}
        <footer className="page-footer">
          <span>
            <RadioIcon aria-hidden="true" />
            {last == null ? t("no_telemetry") : t("last_received", { time: clock(last) })}
          </span>
          <span>{t("footer_note")}</span>
        </footer>
      </main>
    </div>
  );
}
