import { ActivityIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { CartesianGrid, Line, LineChart, XAxis, YAxis } from "recharts";
import ReliabilityTrend from "@/components/reliability-trend";
import TokenTrend from "@/components/token-trend";
import { ChartContainer, ChartTooltip, ChartTooltipContent } from "@/components/ui/chart";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia } from "@/components/ui/empty";
import { clock, type Snapshot, timeTick } from "@/lib/api";
export default function Trend({
  data,
  metric,
}: {
  data: Snapshot;
  metric: "ttft_ms" | "decode_tps" | "tokens" | "reliability";
}) {
  const { t } = useTranslation();
  if (metric === "tokens") return <TokenTrend data={data} />;
  if (metric === "reliability") return <ReliabilityTrend data={data} />;
  const latency = metric === "ttft_ms";
  const values = data.trend.map((p) => ({
    ...p,
    value: latency ? (p.ttft_ms == null ? null : p.ttft_ms / 1000) : p.decode_tps,
  }));
  const hasData = values.some((p) => p.value != null);
  if (!hasData)
    return (
      <Empty className="chart-empty">
        <EmptyHeader>
          <EmptyMedia variant="icon">
            <ActivityIcon />
          </EmptyMedia>
          <EmptyDescription>{t(latency ? "waiting_ttft" : "waiting_decode")}</EmptyDescription>
        </EmptyHeader>
      </Empty>
    );
  return (
    <ChartContainer
      className="trend-chart"
      config={{
        value: {
          label: latency ? t("ttft") : t("decode_estimated"),
          color: latency ? "var(--chart-1)" : "var(--chart-2)",
        },
      }}
    >
      <LineChart
        accessibilityLayer
        data={values}
        margin={{ left: 0, right: 8, top: 12, bottom: 0 }}
      >
        <CartesianGrid vertical={false} stroke="var(--border)" strokeDasharray="3 6" />
        <XAxis
          dataKey="time_ms"
          type="number"
          domain={["dataMin", "dataMax"]}
          tickFormatter={timeTick}
          tickLine={false}
          axisLine={false}
          minTickGap={45}
          tickMargin={12}
        />
        <YAxis
          tickLine={false}
          axisLine={false}
          width={42}
          tickFormatter={(v) => `${v}`}
          domain={[0, "auto"]}
        />
        <ChartTooltip
          content={
            <ChartTooltipContent
              labelFormatter={(_, payload) => clock(payload[0]?.payload.time_ms)}
              formatter={(v) => (
                <span>
                  {Number(v).toFixed(latency ? 2 : 1)} {latency ? "s" : "tok/s"}
                </span>
              )}
            />
          }
        />
        <Line
          type="monotoneX"
          dataKey="value"
          stroke="var(--color-value)"
          strokeWidth={2.4}
          dot={{ r: 3, fill: "var(--color-value)", strokeWidth: 0 }}
          activeDot={{ r: 5 }}
          connectNulls
          isAnimationActive={false}
        />
      </LineChart>
    </ChartContainer>
  );
}
