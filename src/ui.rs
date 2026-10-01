use crate::{models::ModelStats, monitor::Monitor};
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Cell, Paragraph, Row, Table, TableState, Wrap},
};

pub fn draw(
    frame: &mut Frame,
    monitor: &Monitor,
    models: &[ModelStats<'_>],
    selected: usize,
    hours: u32,
    turn_offset: usize,
) {
    let regions = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length((models.len() + 3).clamp(4, 10) as u16),
        Constraint::Length(5),
        Constraint::Min(5),
        Constraint::Length(5),
    ])
    .split(frame.area());
    let window = if hours == 0 {
        "all loaded history".into()
    } else {
        format!("last {hours}h")
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
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
                m.unfinished.to_string(),
                m.failed.to_string(),
                number(m.latest_output_tps),
                seconds(m.latest_first_output_ms.map(|v| v as f64)),
            ]);
        }
        Row::new(cells)
    });
    let mut widths = vec![
        Constraint::Min(20),
        Constraint::Length(5),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(10),
    ];
    let mut columns = vec![
        "Model",
        "Done",
        "First P50",
        "First P95",
        "TPS P50",
        "Non-R P50",
    ];
    if wide {
        widths.extend([
            Constraint::Length(4),
            Constraint::Length(4),
            Constraint::Length(9),
            Constraint::Length(10),
        ]);
        columns.extend(["Open", "Fail", "Last TPS", "Last First"]);
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
        let latest_time = model
            .latest_completed_at
            .map(|d| {
                d.with_timezone(&chrono::Local)
                    .format("%m-%d %H:%M:%S")
                    .to_string()
            })
            .unwrap_or_else(|| "—".into());
        let summary = format!(
            "Latest: {latest_time} · TPS {} · First {}\nValid samples TPS/First/Non-R={}/{}/{}{}\nOpen={} · Failed={} · Interrupted={}",
            number(model.latest_output_tps),
            seconds(model.latest_first_output_ms.map(|v| v as f64)),
            model.tps_samples,
            model.latency_samples,
            model.visible_tps_samples,
            if model.latency_samples < 20 {
                " · small latency sample"
            } else {
                ""
            },
            model.unfinished,
            model.failed,
            model.interrupted,
        );
        frame.render_widget(
            Paragraph::new(summary)
                .wrap(Wrap { trim: true })
                .block(Block::bordered().title(" Model details · local time ")),
            regions[2],
        );
        let rows = model.turns.iter().skip(turn_offset).map(|t| {
            Row::new(vec![
                t.finished_at
                    .or(t.started_at)
                    .map(|d| {
                        d.with_timezone(&chrono::Local)
                            .format("%m-%d %H:%M")
                            .to_string()
                    })
                    .unwrap_or_else(|| "—".into()),
                if t.status == "running" {
                    "open".into()
                } else {
                    t.status.clone()
                },
                seconds(t.duration_ms.map(|v| v as f64)),
                t.usage
                    .as_ref()
                    .map(|u| u.output_tokens.to_string())
                    .unwrap_or_else(|| "—".into()),
                seconds(t.first_output_ms.map(|v| v as f64)),
                number(t.average_tps()),
                number(t.visible_tps()),
            ])
        });
        let widths = [
            Constraint::Length(12),
            Constraint::Length(11),
            Constraint::Length(8),
            Constraint::Length(7),
            Constraint::Length(8),
            Constraint::Length(9),
            Constraint::Min(9),
        ];
        frame.render_widget(
            Table::new(rows, widths)
                .header(header([
                    "Time Local",
                    "Status",
                    "Duration",
                    "Output",
                    "First",
                    "Avg tok/s",
                    "Non-R TPS",
                ]))
                .block(Block::bordered().title(format!(
                    " {} · recent turns · row {}/{} · PgUp/PgDn scroll ",
                    model.model,
                    turn_offset + 1,
                    model.turns.len()
                ))),
            regions[3],
        );
    } else {
        frame.render_widget(Paragraph::new(format!("No model samples in this window. Try --hours 0.\nReading: {} · loaded {} session files.\nSupported sources: cli/exec/vscode; uncompressed JSONL only.", monitor.home.display(), monitor.sessions().len()))
            .wrap(Wrap {trim:true}).block(Block::bordered().title(" Waiting for model samples ")), regions[3]);
    }
    let malformed: usize = monitor.sessions().iter().map(|s| s.malformed_lines).sum();
    let footer = format!(
        "1=1h 2=24h 3=7d 4=all · j/k models · PgUp/PgDn turns · r refresh · q quit\nTPS = whole-turn average, including tools. Non-R = output minus reasoning.\nFirst includes reasoning/tools. Loaded {}/{} files, last 100 turns/file. {}",
        monitor.sessions().len(),
        monitor.file_limit(),
        monitor.warning.clone().unwrap_or_else(|| if malformed > 0 {
            format!("Skipped {malformed} bad lines.")
        } else {
            String::new()
        }),
    );
    frame.render_widget(
        Paragraph::new(footer)
            .style(Style::default().fg(Color::Gray))
            .wrap(Wrap { trim: true })
            .block(Block::bordered()),
        regions[4],
    );
}

fn header<const N: usize>(values: [&str; N]) -> Row<'_> {
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
            .draw(|frame| draw(frame, &monitor, &models, 0, 24, 0))
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
        assert!(content.contains("First includes reasoning/tools"));
        assert!(!content.contains("my-project"));
    }
}
