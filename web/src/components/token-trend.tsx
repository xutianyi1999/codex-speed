import { ActivityIcon } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { CartesianGrid, Line, LineChart, XAxis, YAxis } from "recharts";
import {
  ChartContainer,
  ChartLegend,
  ChartTooltip,
  ChartTooltipContent,
} from "@/components/ui/chart";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia } from "@/components/ui/empty";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { clock, type Snapshot, timeTick, tokens } from "@/lib/api";

export default function TokenTrend({ data }: { data: Snapshot }) {
  const { t } = useTranslation();
  const series = {
    input_tokens: { label: t("input"), color: "var(--chart-1)" },
    cached_input_tokens: { label: t("cached_input"), color: "var(--chart-2)" },
    output_tokens: { label: t("output"), color: "var(--chart-3)" },
    reasoning_output_tokens: { label: t("reasoning_output"), color: "var(--chart-4)" },
  };
  const [selected, setSelected] = useState("all");
  if (
    !data.trend.some(
      (p) =>
        p.input_tokens != null ||
        p.cached_input_tokens != null ||
        p.output_tokens != null ||
        p.reasoning_output_tokens != null,
    )
  ) {
    return (
      <Empty className="chart-empty">
        <EmptyHeader>
          <EmptyMedia variant="icon">
            <ActivityIcon />
          </EmptyMedia>
          <EmptyDescription>{t("waiting_tokens")}</EmptyDescription>
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
          tickFormatter={timeTick}
          tickLine={false}
          axisLine={false}
          minTickGap={45}
          tickMargin={12}
        />
        <YAxis
          tickLine={false}
          axisLine={false}
          width={54}
          tickFormatter={tokens}
          domain={[0, "auto"]}
        />
        <ChartTooltip
          content={
            <ChartTooltipContent
              labelFormatter={(_, payload) => clock(payload[0]?.payload.time_ms)}
              formatter={(value, name) => (
                <div className="flex w-full justify-between gap-6">
                  <span>{series[name as keyof typeof series]?.label}</span>
                  <span className="font-mono tabular-nums">
                    {Number(value).toLocaleString("en")} tokens
                  </span>
                </div>
              )}
            />
          }
        />
        <ChartLegend
          content={
            <ToggleGroup
              aria-label={t("shown_tokens")}
              className="token-legend mx-auto"
              size="sm"
              value={[selected]}
              onValueChange={(value) => {
                if (value[0]) setSelected(value[0]);
              }}
            >
              <ToggleGroupItem value="all">{t("all")}</ToggleGroupItem>
              {Object.entries(series).map(([key, { label, color }]) => (
                <ToggleGroupItem key={key} value={key}>
                  <span
                    aria-hidden="true"
                    className="size-2 shrink-0 rounded-xs"
                    style={{ backgroundColor: color }}
                  />
                  {label}
                </ToggleGroupItem>
              ))}
            </ToggleGroup>
          }
        />
        {Object.keys(series).map((key) => (
          <Line
            key={key}
            dataKey={key}
            hide={selected !== "all" && selected !== key}
            type="monotoneX"
            stroke={`var(--color-${key})`}
            strokeWidth={2.2}
            strokeDasharray={
              key === "cached_input_tokens"
                ? "5 3"
                : key === "reasoning_output_tokens"
                  ? "2 3"
                  : undefined
            }
            dot={{ r: 3, strokeWidth: 0, fill: `var(--color-${key})` }}
            activeDot={{ r: 5 }}
            connectNulls
            isAnimationActive={false}
          />
        ))}
      </LineChart>
    </ChartContainer>
  );
}
