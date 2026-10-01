import { ActivityIcon } from "lucide-react";
import { CartesianGrid, Line, LineChart, XAxis, YAxis } from "recharts";
import TokenTrend from "@/components/token-trend";
import { ChartContainer, ChartTooltip, ChartTooltipContent } from "@/components/ui/chart";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia } from "@/components/ui/empty";
import { clock, type Snapshot } from "@/lib/api";
export default function Trend({
  data,
  metric,
}: {
  data: Snapshot;
  metric: "ttft_ms" | "decode_tps" | "tokens";
}) {
  if (metric === "tokens") return <TokenTrend data={data} />;
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
          <EmptyDescription>等待{latency ? "首 token" : "Decode"}计时数据</EmptyDescription>
        </EmptyHeader>
      </Empty>
    );
  return (
    <ChartContainer
      className="trend-chart"
      config={{
        value: {
          label: latency ? "首 token 延迟" : "估算 Decode 吞吐",
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
          tickFormatter={(v) =>
            new Date(v).toLocaleTimeString("zh-CN", {
              hour: "2-digit",
              minute: "2-digit",
              hour12: false,
            })
          }
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
