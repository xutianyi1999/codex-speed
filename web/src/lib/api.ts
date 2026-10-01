import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";

export interface Distribution {
  samples: number;
  mean_ms: number | null;
  p50_ms: number | null;
  p95_ms: number | null;
  quantiles_approximate: boolean;
}
export interface Summary {
  ttft: Distribution;
  tbt: Distribution;
  decode_tps: number | null;
  input_tokens: number | null;
  cached_input_tokens: number | null;
  output_tokens: number | null;
  reasoning_output_tokens: number | null;
  cached_input_percent: number | null;
  http_attempts: Attempts | null;
  websocket_send_attempts: Attempts | null;
  token_samples: Record<string, number>;
  last_observation_ms: number | null;
  details: Record<string, Distribution>;
}
export interface Attempts {
  total: number;
  failed: number;
  failure_percent: number;
}
export interface Snapshot {
  now_ms: number;
  last_received_ms: number | null;
  window_minutes: number;
  endpoint: string;
  selected_model: string | null;
  models: (Summary & { model: string })[];
  summary: Summary;
  trend: {
    time_ms: number;
    ttft_ms: number | null;
    decode_tps: number | null;
    input_tokens: number | null;
    cached_input_tokens: number | null;
    output_tokens: number | null;
    reasoning_output_tokens: number | null;
    http_failure_percent: number | null;
    websocket_send_failure_percent: number | null;
  }[];
}
export function percent(value: number | null | undefined): string {
  return value == null ? "—" : `${value.toFixed(1)}%`;
}
export function useMetrics(minutes: number, model: string | null) {
  const client = useQueryClient();
  const [connected, setConnected] = useState(false);
  useEffect(() => {
    const source = new EventSource("/api/events");
    source.onopen = () => setConnected(true);
    source.onerror = () => setConnected(false);
    source.addEventListener("metrics", () => {
      void client.invalidateQueries({ queryKey: ["metrics"] });
    });
    return () => source.close();
  }, [client]);
  const query = useQuery({
    queryKey: ["metrics", minutes, model],
    queryFn: async ({ signal }): Promise<Snapshot> => {
      const params = new URLSearchParams({ minutes: String(minutes) });
      if (model != null) params.set("model", model);
      const response = await fetch(`/api/snapshot?${params}`, { signal });
      if (!response.ok) throw new Error("无法获取监控数据，请确认 codex-speed 正在运行。");
      return response.json();
    },
    refetchInterval: 10_000,
    retry: 2,
  });
  return { ...query, connected };
}
export function tokens(value: number | null | undefined): string {
  return value == null
    ? "—"
    : new Intl.NumberFormat("en", { notation: "compact", maximumFractionDigits: 2 }).format(value);
}
export function seconds(value: number | null | undefined): string {
  return value == null ? "—" : (value / 1000).toFixed(2);
}
export function duration(value: number | null | undefined): string {
  if (value == null) return "—";
  return value >= 1000 ? `${(value / 1000).toFixed(2)} s` : `${value.toFixed(2)} ms`;
}
export function rate(value: number | null | undefined): string {
  return value == null ? "—" : value.toFixed(1);
}
export function clock(value: number | null | undefined): string {
  return value == null ? "—" : new Date(value).toLocaleTimeString("zh-CN", { hour12: false });
}
