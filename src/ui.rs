use crate::{models::ModelStats, monitor::Monitor};
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Axis, Block, Cell, Chart, Dataset, GraphType, Paragraph, Row, Table, TableState, Wrap,
    },
};

pub fn draw(
    frame: &mut Frame,
    monitor: &Monitor,
    models: &[ModelStats<'_>],
    selected: usize,
    hours: u32,
    turn_offset: usize,
    show_charts: bool,
) {
    let charts_visible = show_charts && frame.area().height >= 36 && frame.area().width >= 100;
    let regions = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(
            (models.len() + 3).clamp(4, if frame.area().height < 30 { 6 } else { 10 }) as u16,
        ),
        Constraint::Length(if frame.area().width >= 125 { 2 } else { 3 }),
        Constraint::Length(if charts_visible { 9 } else { 0 }),
        Constraint::Min(5),
        Constraint::Length(5),
    ])
    .split(frame.area());
    let window = match hours {
        0 => "loaded history".into(),
        168 => "last 7d".into(),
        _ => format!("last {hours}h"),
    };
    let partial = monitor.skipped_files > 0
        || monitor.warning.is_some()
        || monitor
            .sessions()
            .iter()
            .any(|s| s.dropped_turns > 0 || s.malformed_lines > 0);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                if partial { " PARTIAL · " } else { "" },
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " Codex Speed ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(
                "{} models · {window} · refreshed {}{}",
                models.len(),
                monitor
                    .last_refresh
                    .map(|t| t
                        .with_timezone(&chrono::Local)
                        .format("%H:%M:%S")
                        .to_string())
                    .unwrap_or_else(|| "—".into()),
                if monitor.demo_mode { " · DEMO" } else { "" }
            )),
        ]))
        .block(Block::bordered()),
        regions[0],
    );
    let wide = frame.area().width >= 125;
    let rows = models.iter().map(|m| {
        let mut cells = vec![
            m.model.clone(),
            m.completed.to_string(),
            seconds(m.first_output_p50_ms),
            seconds(m.first_output_p95_ms),
            number(m.output_tps_p50),
            number(m.visible_tps_p50),
        ];
        if wide {
            cells.extend([
                m.failed.to_string(),
                number(m.latest_output_tps),
                seconds(m.latest_first_output_ms.map(|v| v as f64)),
            ]);
        }
        Row::new(cells)
    });
    let mut widths = vec![
        Constraint::Length(22),
        Constraint::Length(5),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(12),
        Constraint::Length(10),
    ];
    let mut columns = vec![
        "Model",
        "Done",
        "First P50",
        "First P95",
        "Turn TPS P50",
        "Non-R P50",
    ];
    if wide {
        widths.extend([
            Constraint::Length(4),
            Constraint::Length(9),
            Constraint::Length(10),
        ]);
        columns.extend(["Fail", "Last TPS", "Last First"]);
    }
    let table = Table::new(rows, widths)
        .header(
            Row::new(columns).style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        )
        .block(Block::bordered().title(" Models · ↑/↓ select "))
        .row_highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White))
        .highlight_symbol("› ");
    let mut state = TableState::default().with_selected((!models.is_empty()).then_some(selected));
    frame.render_stateful_widget(table, regions[1], &mut state);

    if let Some(model) = models.get(selected) {
        let local_time = |time: Option<chrono::DateTime<chrono::Utc>>| {
            time.map(|d| {
                d.with_timezone(&chrono::Local)
                    .format("%m-%d %H:%M")
                    .to_string()
            })
            .unwrap_or_else(|| "—".into())
        };
        let earliest = model
            .turns
            .iter()
            .filter(|t| t.status == "completed")
            .filter_map(|t| t.finished_at.or(t.started_at))
            .min();
        let coverage = format!(
            "Samples {} → {}",
            local_time(earliest),
            local_time(model.latest_completed_at)
        );
        let counts = format!(
            "Unfinished {} · Fail {} · Interrupted {}",
            model.unfinished, model.failed, model.interrupted
        );
        let samples = format!(
            "n TPS/First/Non-R={}/{}/{}{}",
            model.tps_samples,
            model.latency_samples,
            model.visible_tps_samples,
            if model.latency_samples < 20 {
                " (small n)"
            } else {
                ""
            }
        );
        let summary = if wide {
            format!("{coverage} | {samples} | {counts}")
        } else {
            format!(
                "{coverage} | Unfin/Fail/Interrupt={}/{}/{}\n{samples} | Last TPS {} · First {}",
                model.unfinished,
                model.failed,
                model.interrupted,
                number(model.latest_output_tps),
                seconds(model.latest_first_output_ms.map(|v| v as f64))
            )
        };
        let summary = format!(
            "{summary}\nInput {} · Cached {} · n Input/Cache={}/{}",
            tokens(model.total_input_tokens),
            tokens(model.total_cached_input_tokens),
            model.input_token_samples,
            model.cached_input_token_samples,
        );
        frame.render_widget(
            Paragraph::new(summary).style(Style::default().fg(Color::Gray)),
            regions[2],
        );
        if charts_visible {
            draw_trends(frame, regions[3], model);
        }
        let rows = model.turns.iter().skip(turn_offset).map(|t| {
            let mut cells = vec![
                t.finished_at
                    .or(t.started_at)
                    .map(|d| {
                        d.with_timezone(&chrono::Local)
                            .format("%m-%d %H:%M")
                            .to_string()
                    })
                    .unwrap_or_else(|| "—".into()),
                if t.status == "running" {
                    "unfinished".into()
                } else {
                    t.status.clone()
                },
                seconds(t.duration_ms.map(|v| v as f64)),
            ];
            if wide {
                cells.extend([tokens(t.input_tokens()), tokens(t.cached_input_tokens())]);
            } else {
                cells.push(format!(
                    "{}/{}",
                    tokens(t.input_tokens()),
                    tokens(t.cached_input_tokens())
                ));
            }
            cells.extend([
                tokens(t.usage.as_ref().map(|u| u.output_tokens)),
                seconds(t.first_output_ms.map(|v| v as f64)),
                number(t.average_tps()),
                number(t.visible_tps()),
            ]);
            Row::new(cells)
        });
        let (widths, columns) = if wide {
            (
                vec![
                    Constraint::Length(12),
                    Constraint::Length(11),
                    Constraint::Length(8),
                    Constraint::Length(12),
                    Constraint::Length(12),
                    Constraint::Length(7),
                    Constraint::Length(12),
                    Constraint::Length(8),
                    Constraint::Min(9),
                ],
                vec![
                    "Time Local",
                    "Status",
                    "Duration",
                    "Input",
                    "Cached",
                    "Output",
                    "First output",
                    "Turn TPS",
                    "Non-R TPS",
                ],
            )
        } else {
            (
                vec![
                    Constraint::Length(11),
                    Constraint::Length(11),
                    Constraint::Length(6),
                    Constraint::Length(15),
                    Constraint::Length(6),
                    Constraint::Length(7),
                    Constraint::Length(7),
                    Constraint::Min(7),
                ],
                vec![
                    "Time Local",
                    "Status",
                    "Dur.",
                    "Input/Cache",
                    "Output",
                    "First",
                    "TPS",
                    "Non-R",
                ],
            )
        };
        frame.render_widget(
            Table::new(rows, widths)
                .header(header(columns))
                .block(Block::bordered().title(format!(
                    " {} · recent turns · row {}/{} · PgUp/PgDn scroll ",
                    model.model,
                    turn_offset + 1,
                    model.turns.len()
                ))),
            regions[4],
        );
    } else {
        frame.render_widget(Paragraph::new(format!("No model samples in this window. Try --hours 0.\nReading: {} · loaded {} session files.\nSupported sources: cli/exec/vscode; uncompressed JSONL only.", monitor.home.display(), monitor.sessions().len()))
            .wrap(Wrap {trim:true}).block(Block::bordered().title(" Waiting for model samples ")), regions[4]);
    }
    let malformed: usize = monitor.sessions().iter().map(|s| s.malformed_lines).sum();
    let dropped: usize = monitor.sessions().iter().map(|s| s.dropped_turns).sum();
    let footer = format!(
        "1=1h 2=24h 3=7d 4=all · j/k select · PgUp/PgDn scroll · c charts · r · q\nTPS = whole-turn average, including tools. Non-R = output minus reasoning.\nFirst includes reasoning/tools. {} files · {} {}",
        monitor.sessions().len(),
        if monitor.skipped_files > 0 || dropped > 0 {
            format!(
                "omitted {} files / {} turns",
                monitor.skipped_files, dropped
            )
        } else {
            "none omitted".into()
        },
        monitor
            .warning
            .as_ref()
            .map(|w| format!("PARTIAL: {w}"))
            .unwrap_or_else(|| if malformed > 0 {
                format!("PARTIAL: skipped {malformed} bad lines.")
            } else {
                String::new()
            }),
    );
    frame.render_widget(
        Paragraph::new(footer)
            .style(Style::default().fg(Color::Gray))
            .wrap(Wrap { trim: true })
            .block(Block::bordered()),
        regions[5],
    );
}

fn draw_trends(frame: &mut Frame, area: ratatui::layout::Rect, model: &ModelStats<'_>) {
    // One shared ordinal axis; missing values stay absent rather than becoming zero.
    let mut turns: Vec<_> = model
        .turns
        .iter()
        .filter(|t| t.status == "completed")
        .take(30)
        .copied()
        .collect();
    turns.reverse();
    let output: Vec<_> = turns
        .iter()
        .enumerate()
        .filter_map(|(i, t)| t.average_tps().map(|v| (i as f64, v)))
        .collect();
    let non_reasoning: Vec<_> = turns
        .iter()
        .enumerate()
        .filter_map(|(i, t)| t.visible_tps().map(|v| (i as f64, v)))
        .collect();
    let first: Vec<_> = turns
        .iter()
        .enumerate()
        .filter_map(|(i, t)| t.first_output_ms.map(|v| (i as f64, v as f64 / 1000.0)))
        .collect();
    let panels =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(area);
    let x_max = turns.len().saturating_sub(1).max(1) as f64;
    let tps_reference: Vec<_> = model
        .output_tps_p50
        .map(|v| vec![(0.0, v), (x_max, v)])
        .unwrap_or_default();
    let first_reference: Vec<_> = model
        .first_output_p50_ms
        .map(|v| vec![(0.0, v / 1000.0), (x_max, v / 1000.0)])
        .unwrap_or_default();
    let tps_max = output
        .iter()
        .chain(&non_reasoning)
        .chain(&tps_reference)
        .map(|p| p.1)
        .fold(1.0, f64::max)
        * 1.1;
    let first_max = first
        .iter()
        .chain(&first_reference)
        .map(|p| p.1)
        .fold(1.0, f64::max)
        * 1.1;
    let tps_title = format!(
        " Turn TPS ↑ · cyan Total / yellow Non-R · window P50 {} ",
        number(model.output_tps_p50)
    );
    let first_title = format!(
        " First output (s) ↓ · window P50 {} ",
        seconds(model.first_output_p50_ms)
    );
    let oldest_label = format!("oldest #{}", turns.len().max(1));
    for (panel, title, datasets, y_max) in [
        (
            panels[0],
            tps_title.as_str(),
            vec![
                Dataset::default()
                    .data(&tps_reference)
                    .graph_type(GraphType::Line)
                    .marker(ratatui::symbols::Marker::Dot)
                    .style(Style::default().fg(Color::Gray)),
                Dataset::default()
                    .name("Total")
                    .data(&output)
                    .marker(ratatui::symbols::Marker::Dot)
                    .graph_type(GraphType::Scatter)
                    .style(Style::default().fg(Color::Cyan)),
                Dataset::default()
                    .name("Non-R")
                    .data(&non_reasoning)
                    .marker(ratatui::symbols::Marker::Dot)
                    .graph_type(GraphType::Scatter)
                    .style(Style::default().fg(Color::Yellow)),
            ],
            tps_max,
        ),
        (
            panels[1],
            first_title.as_str(),
            vec![
                Dataset::default()
                    .data(&first_reference)
                    .graph_type(GraphType::Line)
                    .marker(ratatui::symbols::Marker::Dot)
                    .style(Style::default().fg(Color::Gray)),
                Dataset::default()
                    .data(&first)
                    .marker(ratatui::symbols::Marker::Dot)
                    .graph_type(GraphType::Scatter)
                    .style(Style::default().fg(Color::Green)),
            ],
            first_max,
        ),
    ] {
        if (panel == panels[0] && output.is_empty() && non_reasoning.is_empty())
            || (panel == panels[1] && first.is_empty())
        {
            frame.render_widget(
                Paragraph::new("No valid samples").block(Block::bordered().title(title)),
                panel,
            );
            continue;
        }
        frame.render_widget(
            Chart::new(datasets)
                .block(Block::bordered().title(title))
                .x_axis(
                    Axis::default()
                        .bounds([0.0, x_max])
                        .labels([oldest_label.as_str(), "newest #1"]),
                )
                .y_axis(
                    Axis::default()
                        .bounds([0.0, y_max])
                        .labels(["0".to_owned(), format!("{y_max:.1}")]),
                ),
            panel,
        );
    }
}

fn header<'a>(values: impl IntoIterator<Item = &'a str>) -> Row<'a> {
    Row::new(values.into_iter().map(Cell::from)).style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )
}
fn seconds(ms: Option<f64>) -> String {
    ms.map(|v| format!("{:.2}s", v / 1000.0))
        .unwrap_or_else(|| "—".into())
}
fn tokens(value: Option<u64>) -> String {
    let Some(value) = value else {
        return "—".into();
    };
    if value < 1_000 {
        return value.to_string();
    }
    // Decimal token units; rounding near a boundary promotes to the next unit.
    let units = ["", "K", "M", "B", "T", "P", "E"];
    let mut scaled = value as f64;
    let mut unit = 0;
    while scaled >= 999.95 && unit + 1 < units.len() {
        scaled /= 1_000.0;
        unit += 1;
    }
    if scaled < 10.0 {
        format!("{scaled:.2}{}", units[unit])
    } else {
        format!("{scaled:.1}{}", units[unit])
    }
}
fn number(value: Option<f64>) -> String {
    value
        .map(|v| format!("{v:.1}"))
        .unwrap_or_else(|| "—".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
    #[test]
    fn model_dashboard_keeps_metric_scope_visible_on_standard_terminal() {
        let mut monitor = Monitor::new("unused".into(), 50);
        monitor.demo();
        let models = monitor.models(24);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| draw(frame, &monitor, &models, 0, 24, 0, true))
            .unwrap();
        let content: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(content.contains("Models"));
        assert!(content.contains("demo-model-a"));
        assert!(content.contains("First P95"));
        assert!(content.contains("38.6"));
        assert!(content.contains("Input/Cache"));
        assert!(content.contains("12.0K/9.00K"));
        assert!(content.contains("Input 12.0K · Cached 9.00K"));
        assert!(content.contains("First includes reasoning/tools"));
        assert!(!content.contains("my-project"));
    }

    #[test]
    fn wide_terminal_displays_charts_and_can_hide_them() {
        let mut monitor = Monitor::new("unused".into(), 0);
        monitor.demo();
        let models = monitor.models(168);
        let mut terminal = Terminal::new(TestBackend::new(150, 45)).unwrap();
        for visible in [true, false] {
            terminal
                .draw(|frame| draw(frame, &monitor, &models, 0, 168, 0, visible))
                .unwrap();
            let content: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect();
            assert!(content.contains("last 7d"));
            assert!(content.contains("Last First"));
            assert!(content.contains("Samples "));
            assert!(content.contains("Input 12.0K · Cached 9.00K"));
            assert!(content.contains("Input"));
            assert!(content.contains("Cached"));
            assert!(!content.contains("Model details"));
            assert!(content.contains("Unfinished"));
            assert_eq!(content.contains("First output (s)"), visible);
            assert_eq!(content.contains("window P50"), visible);
        }
    }
}
