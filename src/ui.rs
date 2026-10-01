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
) {
    let regions = Layout::vertical([
        Constraint::Length(3),
        Constraint::Percentage(40),
        Constraint::Min(5),
        Constraint::Length(4),
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
                "{} models · {window}{}",
                models.len(),
                if monitor.demo_mode { " · DEMO" } else { "" }
            )),
        ]))
        .block(Block::bordered()),
        regions[0],
    );
    let rows = models.iter().map(|m| {
        Row::new(vec![
            m.model.clone(),
            m.completed.to_string(),
            seconds(m.first_output_p50_ms),
            seconds(m.first_output_p95_ms),
            number(m.output_tps_p50),
            number(m.visible_tps_p50),
        ])
    });
    let widths = [
        Constraint::Min(20),
        Constraint::Length(5),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(10),
    ];
    let table = Table::new(rows, widths)
        .header(header([
            "Model",
            "Done",
            "First P50",
            "First P95",
            "TPS P50",
            "Text P50",
        ]))
        .block(Block::bordered().title(" Models · ↑/↓ select "))
        .row_highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White))
        .highlight_symbol("› ");
    let mut state = TableState::default().with_selected((!models.is_empty()).then_some(selected));
    frame.render_stateful_widget(table, regions[1], &mut state);

    if let Some(model) = models.get(selected) {
        let rows = model.turns.iter().map(|t| {
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
                    "Text tok/s",
                ]))
                .block(Block::bordered().title(format!(
                    " {} · samples TPS/First/Text={}/{}/{} · excluded={} open={} ",
                    model.model,
                    model.tps_samples,
                    model.latency_samples,
                    model.visible_tps_samples,
                    model.excluded,
                    model.unfinished
                ))),
            regions[2],
        );
    } else {
        frame.render_widget(Paragraph::new(format!("No model samples in this window. Try --hours 0.\nReading: {} · loaded {} session files.\nSupported sources: cli/exec/vscode; uncompressed JSONL only.", monitor.home.display(), monitor.sessions().len()))
            .wrap(Wrap {trim:true}).block(Block::bordered().title(" Waiting for model samples ")), regions[2]);
    }
    let malformed: usize = monitor.sessions().iter().map(|s| s.malformed_lines).sum();
    let footer = format!(
        "q quit · r refresh · j/k select | TPS = whole-turn average, including tools.\nFirst includes reasoning/tools. Failed/interrupted excluded. {}",
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
        regions[3],
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
            .draw(|frame| draw(frame, &monitor, &models, 0, 24))
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
