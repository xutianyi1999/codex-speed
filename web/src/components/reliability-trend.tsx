import { ActivityIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { CartesianGrid, Line, LineChart, XAxis, YAxis } from "recharts";
import { ChartContainer, ChartTooltip, ChartTooltipContent } from "@/components/ui/chart";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia } from "@/components/ui/empty";
import { chartTime, percent, type Snapshot, trendTimeTick } from "@/lib/api";

export default function ReliabilityTrend({ data }: { data: Snapshot }) {
  const { t } = useTranslation();
  const series = {
    http_failure_percent: { label: t("http_failure"), color: "var(--chart-1)" },
    websocket_send_failure_percent: { label: t("ws_failure"), color: "var(--chart-3)" },
  };
  if (
    !data.trend.some(
      (p) => p.http_failure_percent != null || p.websocket_send_failure_percent != null,
    )
  ) {
    return (
      <Empty className="chart-empty">
        <EmptyHeader>
          <EmptyMedia variant="icon">
            <ActivityIcon />
          </EmptyMedia>
          <EmptyDescription>{t("waiting_attempts")}</EmptyDescription>
        </EmptyHeader>
      </Empty>
    );
  }
  return (
    <ChartContainer className="trend-chart" config={series}>
      <LineChart
        accessibilityLayer
        data={data.trend}
        margin={{ left: 0, right: 8, top: 12, bottom: 0 }}
      >
        <CartesianGrid vertical={false} stroke="var(--border)" strokeDasharray="3 6" />
        <XAxis
          dataKey="time_ms"
          type="number"
          domain={["dataMin", "dataMax"]}
          tickFormatter={trendTimeTick(data.trend)}
          tickLine={false}
          axisLine={false}
          minTickGap={45}
          interval="preserveStartEnd"
          tickMargin={12}
        />
        <YAxis
          tickLine={false}
          axisLine={false}
          width={54}
          domain={[0, 100]}
          ticks={[0, 25, 50, 75, 100]}
          tickFormatter={(v) => `${v}%`}
        />
        <ChartTooltip
          content={
            <ChartTooltipContent
              labelFormatter={(_, payload) => chartTime(payload[0]?.payload.time_ms)}
              formatter={(value, name) => (
                <div className="flex w-full justify-between gap-6">
                  <span>{series[name as keyof typeof series]?.label}</span>
                  <span className="font-mono tabular-nums">{percent(Number(value))}</span>
                </div>
              )}
            />
          }
        />
        {Object.keys(series).map((key) => (
          <Line
            key={key}
            type="monotoneX"
            dataKey={key}
            stroke={`var(--color-${key})`}
            strokeWidth={2.2}
            strokeDasharray={key === "websocket_send_failure_percent" ? "5 3" : undefined}
            dot={{ r: 3, fill: `var(--color-${key})`, strokeWidth: 0 }}
            activeDot={{ r: 5 }}
            connectNulls
            isAnimationActive={false}
          />
        ))}
      </LineChart>
    </ChartContainer>
  );
}
