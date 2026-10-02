import { ActivityIcon, CoinsIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia } from "@/components/ui/empty";
import {
  Table,
  TableBody,
  TableCaption,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  duration,
  percent,
  type RecentReport,
  type ReportKind,
  rate,
  reportCacheShare,
  reportTime,
  type Snapshot,
  tokens,
} from "@/lib/api";

const labels: Record<ReportKind, string> = {
  ttft: "mean_ttft",
  tbt: "estimated_decode",
  input: "input",
  cached_input: "cached_input",
  output: "output",
  reasoning_output: "reasoning_output",
};

function ReportValue({ row, kind }: { row: RecentReport; kind: ReportKind }) {
  const { t } = useTranslation();
  const metric = row.values[kind];
  if (!metric) return <span className="report-missing">—</span>;
  const formatted =
    kind === "ttft"
      ? duration(metric.value)
      : kind === "tbt"
        ? `${rate(metric.value)} tok/s`
        : tokens(metric.value);
  const [value, unit] = formatted.split(" ");
  return (
    <div
      className="report-measurement"
      title={`${metric.value ?? "—"} · ${t("observation_count", { count: metric.samples })}`}
    >
      <span>
        <span className="report-value">{value}</span>
        {unit && (
          <>
            {" "}
            <span className="report-unit">{unit}</span>
          </>
        )}
      </span>
      {metric.samples > 1 && (
        <span className="report-samples" aria-hidden="true">
          n={metric.samples}
        </span>
      )}
      <span className="sr-only">{t("observation_count", { count: metric.samples })}</span>
    </div>
  );
}

function ReportList({ rows, timing }: { rows: RecentReport[]; timing: boolean }) {
  const { t } = useTranslation();
  const title = t(timing ? "recent_timings" : "recent_tokens");
  const id = timing ? "recent-timings" : "recent-tokens";
  const kinds: ReportKind[] = timing
    ? ["ttft", "tbt"]
    : ["input", "cached_input", "output", "reasoning_output"];
  return (
    <section
      className="reports-section"
      data-report-type={timing ? "timing" : "tokens"}
      aria-labelledby={id}
    >
      <Card className="report-card">
        <CardHeader>
          <CardTitle id={id}>{title}</CardTitle>
          <CardDescription>
            {t(timing ? "recent_timings_description" : "recent_tokens_description")}
          </CardDescription>
          <CardAction>
            <Badge variant="secondary">{t("latest_records", { count: rows.length })}</Badge>
          </CardAction>
        </CardHeader>
        <CardContent>
          {rows.length ? (
            <section
              className="report-scroll"
              aria-label={t("report_scroll", { title })}
              // biome-ignore lint/a11y/noNoninteractiveTabindex: Keyboard users must be able to scroll the report table.
              tabIndex={0}
            >
              <Table>
                <TableCaption className="sr-only">{title}</TableCaption>
                <TableHeader>
                  <TableRow>
                    <TableHead scope="col">{t("received_time")}</TableHead>
                    {kinds.map((kind) => (
                      <TableHead scope="col" key={kind}>
                        {t(labels[kind])}
                      </TableHead>
                    ))}
                    {!timing && <TableHead scope="col">{t("report_cache_share")}</TableHead>}
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {rows.map((row) => (
                    <TableRow key={row.id} data-batch={row.id}>
                      <TableCell>
                        <time
                          dateTime={new Date(row.received_ms).toISOString()}
                          title={new Date(row.received_ms).toLocaleString()}
                        >
                          {reportTime(row.received_ms)}
                        </time>
                      </TableCell>
                      {kinds.map((kind) => (
                        <TableCell key={kind} data-kind={kind}>
                          <ReportValue row={row} kind={kind} />
                        </TableCell>
                      ))}
                      {!timing && (
                        <TableCell data-kind="cache_share" title={t("report_cache_definition")}>
                          <span className="report-value">{percent(reportCacheShare(row))}</span>
                        </TableCell>
                      )}
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </section>
          ) : (
            <Empty>
              <EmptyHeader>
                <EmptyMedia variant="icon">{timing ? <ActivityIcon /> : <CoinsIcon />}</EmptyMedia>
                <EmptyDescription>
                  {t(timing ? "waiting_ttft_reports" : "waiting_token_reports")}
                </EmptyDescription>
              </EmptyHeader>
            </Empty>
          )}
        </CardContent>
      </Card>
    </section>
  );
}

export default function RecentReports({ data }: { data: Snapshot }) {
  const { t } = useTranslation();
  return (
    <div className="recent-reports">
      <div className="reports-grid">
        <ReportList timing rows={data.recent_timings} />
        <ReportList timing={false} rows={data.recent_tokens} />
      </div>
      <p className="report-note">{t("reports_note")}</p>
    </div>
  );
}
