use ratatui::{prelude::*, widgets::*};

use crate::app::{App, FocusedPane};

pub fn render(f: &mut Frame, app: &App) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Max(3)])
        .split(f.size());

    if let [content_area, instructions_area] = main_chunks[..] {
        let content_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(content_area);

        if let [monitors_area, options_area] = content_chunks[..] {
            render_monitors_pane(f, app, monitors_area);
            render_options_pane(f, app, options_area);
            render_instructions(f, app, instructions_area);
        }
    }
}

fn render_monitors_pane(f: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.is_focused(FocusedPane::Monitors);

    let items: Vec<ListItem> = app
        .monitors
        .iter()
        .map(|m| {
            let icon = if m.active { "✅" } else { "❌" };
            ListItem::new(format!("{} {}", icon, m.name))
        })
        .collect();

    let list = List::new(items)
        .block(create_block("Monitors", is_focused))
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::Blue),
        )
        .highlight_symbol(">> ");

    f.render_stateful_widget(list, area, &mut app.monitor_list_state.clone());
}

fn render_options_pane(f: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.is_focused(FocusedPane::Options);
    let block = create_block("Options", is_focused);

    let Some(idx) = app.selected_monitor() else {
        f.render_widget(block, area);
        return;
    };

    let config = &app.configs[idx];
    let dpms_status_text = if config.dpms_on { "On" } else { "Off" };

    let items = vec![
        ListItem::new(format!("{:<13} <{}>", "Resolution:", config.resolution)),
        ListItem::new(format!(
            "{:<13} <{:.1} Hz>",
            "Refresh Rate:", config.refresh_rate
        )),
        ListItem::new(format!("{:<13} <{:.2}>", "Scale:", config.scale_as_float())),
        ListItem::new(
            Line::from("-> Apply Changes <-")
                .style(Style::default().fg(Color::Green))
                .alignment(Alignment::Center),
        ),
        ListItem::new(Line::from("Set as Main Screen").alignment(Alignment::Center)),
        ListItem::new(Line::from("Extend Left").alignment(Alignment::Center)),
        ListItem::new(Line::from("Extend Right").alignment(Alignment::Center)),
        ListItem::new(Line::from("Mirror Another Monitor").alignment(Alignment::Center)),
        ListItem::new(
            Line::from(format!(
                "Toggle Black Screen (Currently: {})",
                dpms_status_text
            ))
            .alignment(Alignment::Center),
        ),
        ListItem::new(
            Line::from("-> Save to File <-")
                .style(Style::default().fg(Color::Cyan))
                .alignment(Alignment::Center),
        ),
        ListItem::new(
            Line::from("-> Disable Monitor <-")
                .style(Style::default().fg(Color::Red))
                .alignment(Alignment::Center),
        ),
    ];

    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .add_modifier(Modifier::BOLD)
            .bg(Color::Blue),
    );

    f.render_stateful_widget(list, area, &mut app.option_list_state.clone());
}

fn render_instructions(f: &mut Frame, app: &App, area: Rect) {
    let text = if let Some(msg) = &app.info_message {
        msg.clone()
    } else {
        String::from("Tab: Switch Panes | ↑/↓: Navigate | ←/→: Change Value | Enter: Execute Action | q: Quit")
    };

    let color = if app.info_message.is_some() {
        Color::Cyan
    } else {
        Color::Yellow
    };

    let instructions = Paragraph::new(text)
        .style(Style::default().fg(color))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(Color::Reset)),
        );

    f.render_widget(instructions, area);
}

fn create_block(title: &str, is_focused: bool) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(if is_focused {
            Color::Blue
        } else {
            Color::Reset
        }))
}
