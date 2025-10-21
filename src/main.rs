use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{prelude::*, widgets::*};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    io::{self, stdout},
    process::{Command, Stdio},
};

// --- Constants ---
const SCALE_STEP: i32 = 25;
const MIN_SCALE: i32 = 50;
const OPTION_COUNT: usize = 5;
const APPLY_OPTION_IDX: usize = 3;
const DISABLE_OPTION_IDX: usize = 4;

// --- Data Structures ---
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct Monitor {
    name: String,
    #[serde(default)]
    active: bool,
    #[serde(skip)]
    modes: BTreeMap<String, Vec<f64>>,
}

#[derive(Debug, Clone)]
struct MonitorConfig {
    resolution: String,
    refresh_rate: f64,
    scale: i32,
    resolution_index: usize,
    refresh_rate_index: usize,
}

impl MonitorConfig {
    fn scale_as_float(&self) -> f64 {
        self.scale as f64 / 100.0
    }
}

#[derive(PartialEq)]
enum FocusedPane {
    Monitors,
    Options,
}

struct App {
    monitors: Vec<Monitor>,
    configs: Vec<MonitorConfig>,
    monitor_list_state: ListState,
    option_list_state: ListState,
    focused_pane: FocusedPane,
}

// --- Application Logic ---
impl App {
    fn new() -> io::Result<Self> {
        let monitors_data = Self::fetch_monitors()?;
        let (monitors, configs) = Self::parse_monitors(monitors_data)?;
        let monitor_count = monitors.len();

        Ok(Self {
            monitors,
            configs,
            monitor_list_state: Self::init_list_state(monitor_count),
            option_list_state: Self::init_list_state(OPTION_COUNT),
            focused_pane: FocusedPane::Monitors,
        })
    }

    fn init_list_state(count: usize) -> ListState {
        let mut state = ListState::default();
        if count > 0 {
            state.select(Some(0));
        }
        state
    }

    fn fetch_monitors() -> io::Result<Vec<serde_json::Value>> {
        let output = Command::new("hyprctl")
            .args(["monitors", "all", "-j"])
            .output()?;

        serde_json::from_slice(&output.stdout)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    fn parse_monitors(
        monitors_data: Vec<serde_json::Value>,
    ) -> io::Result<(Vec<Monitor>, Vec<MonitorConfig>)> {
        monitors_data
            .into_iter()
            .filter_map(|data| Self::parse_single_monitor(&data))
            .collect::<Result<Vec<_>, _>>()
            .map(|pairs| pairs.into_iter().unzip())
    }

    fn parse_single_monitor(
        data: &serde_json::Value,
    ) -> Option<io::Result<(Monitor, MonitorConfig)>> {
        let name = data["name"].as_str()?.to_string();
        if name.is_empty() {
            return None;
        }

        let modes = Self::parse_modes(data);
        let active = !data["disabled"].as_bool().unwrap_or(true);
        let scale = Self::parse_scale(data);
        let (res_idx, refresh_idx) = Self::find_current_mode(data, &modes, active);

        let resolutions: Vec<_> = modes.keys().cloned().collect();
        let resolution = resolutions.get(res_idx).cloned().unwrap_or_default();
        let refresh_rate = modes
            .get(&resolution)
            .and_then(|rates| rates.get(refresh_idx).copied())
            .unwrap_or(60.0);

        Some(Ok((
            Monitor {
                name,
                active,
                modes,
            },
            MonitorConfig {
                resolution,
                refresh_rate,
                scale,
                resolution_index: res_idx,
                refresh_rate_index: refresh_idx,
            },
        )))
    }

    fn parse_modes(data: &serde_json::Value) -> BTreeMap<String, Vec<f64>> {
        let mut modes = BTreeMap::new();

        if let Some(available_modes) = data["availableModes"].as_array() {
            for mode_str in available_modes.iter().filter_map(|v| v.as_str()) {
                if let Some((res, rate_str)) = mode_str.split_once('@') {
                    if let Ok(rate) = rate_str.trim_end_matches("Hz").parse::<f64>() {
                        modes
                            .entry(res.to_string())
                            .or_insert_with(Vec::new)
                            .push(rate);
                    }
                }
            }
        }

        // Sort descending and deduplicate
        for rates in modes.values_mut() {
            rates.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
            rates.dedup_by(|a, b| (*a - *b).abs() < 0.01);
        }

        modes
    }

    fn parse_scale(data: &serde_json::Value) -> i32 {
        let scale = data["scale"].as_f64().unwrap_or(1.0).max(0.1);
        (scale * 100.0).round() as i32
    }

    fn find_current_mode(
        data: &serde_json::Value,
        modes: &BTreeMap<String, Vec<f64>>,
        active: bool,
    ) -> (usize, usize) {
        if !active || modes.is_empty() {
            return (0, 0);
        }

        let current_w = data["width"].as_i64().unwrap_or(0);
        let current_h = data["height"].as_i64().unwrap_or(0);
        let current_rate = data["refreshRate"].as_f64().unwrap_or(60.0);

        let res_idx = modes
            .keys()
            .enumerate()
            .min_by_key(|(_, res)| Self::resolution_distance(res, current_w, current_h))
            .map(|(i, _)| i)
            .unwrap_or(0);

        let resolution = modes.keys().nth(res_idx).cloned().unwrap_or_default();
        let refresh_idx = modes
            .get(&resolution)
            .and_then(|rates| {
                rates
                    .iter()
                    .position(|&rate| (rate - current_rate).abs() < 0.1)
            })
            .unwrap_or(0);

        (res_idx, refresh_idx)
    }

    fn resolution_distance(res: &str, target_w: i64, target_h: i64) -> i64 {
        let parts: Vec<i64> = res.split('x').filter_map(|s| s.parse().ok()).collect();
        if parts.len() == 2 {
            (parts[0] - target_w).pow(2) + (parts[1] - target_h).pow(2)
        } else {
            i64::MAX
        }
    }

    fn cycle_selection(current: Option<usize>, max: usize, forward: bool) -> Option<usize> {
        if max == 0 {
            return None;
        }
        Some(match current {
            Some(i) if forward => (i + 1) % max,
            Some(i) => (i + max - 1) % max,
            None => 0,
        })
    }

    fn navigate_monitors(&mut self, forward: bool) {
        let selection = Self::cycle_selection(
            self.monitor_list_state.selected(),
            self.monitors.len(),
            forward,
        );
        self.monitor_list_state.select(selection);
        self.option_list_state.select(Some(0));
    }

    fn navigate_options(&mut self, forward: bool) {
        let selection =
            Self::cycle_selection(self.option_list_state.selected(), OPTION_COUNT, forward);
        self.option_list_state.select(selection);
    }

    fn modify_selected_option(&mut self, increase: bool) {
        let Some(mon_idx) = self.monitor_list_state.selected() else {
            return;
        };
        let Some(opt_idx) = self.option_list_state.selected() else {
            return;
        };

        match opt_idx {
            0 => self.cycle_resolution(mon_idx, increase),
            1 => self.cycle_refresh_rate(mon_idx, increase),
            2 => self.adjust_scale(mon_idx, increase),
            _ => {}
        }
    }

    fn cycle_resolution(&mut self, mon_idx: usize, increase: bool) {
        let resolutions: Vec<_> = self.monitors[mon_idx].modes.keys().cloned().collect();
        if resolutions.is_empty() {
            return;
        }

        let config = &mut self.configs[mon_idx];
        config.resolution_index = if increase {
            (config.resolution_index + 1) % resolutions.len()
        } else {
            (config.resolution_index + resolutions.len() - 1) % resolutions.len()
        };

        config.resolution = resolutions[config.resolution_index].clone();
        config.refresh_rate_index = 0;

        // Update refresh rate to first available for new resolution
        if let Some(rates) = self.monitors[mon_idx].modes.get(&config.resolution) {
            config.refresh_rate = rates.first().copied().unwrap_or(60.0);
        }
    }

    fn cycle_refresh_rate(&mut self, mon_idx: usize, increase: bool) {
        let resolution = self.configs[mon_idx].resolution.clone();
        let Some(rates) = self.monitors[mon_idx].modes.get(&resolution).cloned() else {
            return;
        };
        if rates.is_empty() {
            return;
        }

        let config = &mut self.configs[mon_idx];
        config.refresh_rate_index = if increase {
            (config.refresh_rate_index + 1) % rates.len()
        } else {
            (config.refresh_rate_index + rates.len() - 1) % rates.len()
        };

        config.refresh_rate = rates[config.refresh_rate_index];
    }

    fn adjust_scale(&mut self, mon_idx: usize, increase: bool) {
        let config = &mut self.configs[mon_idx];
        config.scale =
            (config.scale + if increase { SCALE_STEP } else { -SCALE_STEP }).max(MIN_SCALE);
    }

    fn execute_hyprctl(&self, command: &str) -> bool {
        Command::new("sh")
            .arg("-c")
            .arg(command)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    fn apply_changes(&self) {
        let Some(idx) = self.monitor_list_state.selected() else {
            return;
        };

        let monitor = &self.monitors[idx];
        let config = &self.configs[idx];

        let command = format!(
            "hyprctl --batch \"keyword monitor {},reset;keyword monitor {},{},auto,{:.2}\"",
            monitor.name,
            monitor.name,
            config.resolution,
            config.scale_as_float()
        );

        self.execute_hyprctl(&command);
    }

    fn disable_monitor(&self) {
        let Some(idx) = self.monitor_list_state.selected() else {
            return;
        };

        let command = format!(
            "hyprctl keyword monitor \"{},disable\"",
            self.monitors[idx].name
        );

        self.execute_hyprctl(&command);
    }

    fn toggle_pane(&mut self) {
        self.focused_pane = if self.focused_pane == FocusedPane::Monitors {
            FocusedPane::Options
        } else {
            FocusedPane::Monitors
        };
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => return true,
            KeyCode::Tab => self.toggle_pane(),
            KeyCode::Char('j') | KeyCode::Down => match self.focused_pane {
                FocusedPane::Monitors => self.navigate_monitors(true),
                FocusedPane::Options => self.navigate_options(true),
            },
            KeyCode::Char('k') | KeyCode::Up => match self.focused_pane {
                FocusedPane::Monitors => self.navigate_monitors(false),
                FocusedPane::Options => self.navigate_options(false),
            },
            KeyCode::Char('l') | KeyCode::Right if self.focused_pane == FocusedPane::Options => {
                self.modify_selected_option(true)
            }
            KeyCode::Char('h') | KeyCode::Left if self.focused_pane == FocusedPane::Options => {
                self.modify_selected_option(false)
            }
            KeyCode::Enter if self.focused_pane == FocusedPane::Options => {
                match self.option_list_state.selected() {
                    Some(APPLY_OPTION_IDX) => self.apply_changes(),
                    Some(DISABLE_OPTION_IDX) => self.disable_monitor(),
                    _ => {}
                }
            }
            _ => {}
        }
        false
    }

    fn selected_monitor(&self) -> Option<usize> {
        self.monitor_list_state.selected()
    }

    fn is_focused(&self, pane: FocusedPane) -> bool {
        self.focused_pane == pane
    }
}

// --- Main and UI ---
fn main() -> io::Result<()> {
    let mut terminal = setup_terminal()?;
    let result = run_app(&mut terminal);
    restore_terminal()?;
    result
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<std::io::Stdout>>> {
    stdout().execute(EnterAlternateScreen)?;
    enable_raw_mode()?;
    Terminal::new(CrosstermBackend::new(stdout()))
}

fn restore_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> io::Result<()> {
    let mut app = App::new()?;

    loop {
        terminal.draw(|f| ui(f, &app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press && app.handle_key(key.code) {
                break;
            }
        }
    }

    Ok(())
}

fn ui(f: &mut Frame, app: &App) {
    let [content_area, instructions_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .areas(f.size());

    let [monitors_area, options_area] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .areas(content_area);

    render_monitors_pane(f, app, monitors_area);
    render_options_pane(f, app, options_area);
    render_instructions(f, instructions_area);
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

    let items = vec![
        ListItem::new(format!("Resolution:   <{}>", config.resolution)),
        ListItem::new(format!("Refresh Rate: <{:.1} Hz>", config.refresh_rate)),
        ListItem::new(format!("Scale:        <{:.2}>", config.scale_as_float())),
        ListItem::new(Line::from("-> Apply Changes <-").alignment(Alignment::Center)),
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

fn render_instructions(f: &mut Frame, area: Rect) {
    let text =
        "Tab: Switch Panes | ↑/↓: Navigate | ←/→: Change Value | Enter: Apply/Disable | q: Quit";
    let instructions = Paragraph::new(text)
        .style(Style::default().fg(Color::Yellow))
        .alignment(Alignment::Center);

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
