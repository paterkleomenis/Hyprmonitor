// ============================================================================
// src/app.rs
// ============================================================================

use crossterm::event::KeyCode;
use ratatui::widgets::ListState;
use std::{collections::BTreeMap, fs, io, path::PathBuf};

use crate::commands;
use crate::monitor::{Monitor, MonitorConfig};

const SCALE_STEP: i32 = 25;
const MIN_SCALE: i32 = 50;
pub const OPTION_COUNT: usize = 11;

const APPLY_OPTION_IDX: usize = 3;
const SET_MAIN_IDX: usize = 4;
const EXTEND_LEFT_IDX: usize = 5;
const EXTEND_RIGHT_IDX: usize = 6;
const MIRROR_IDX: usize = 7;
const BLACK_SCREEN_IDX: usize = 8;
const SAVE_OPTION_IDX: usize = 9;
const DISABLE_OPTION_IDX: usize = 10;

#[derive(PartialEq)]
pub enum FocusedPane {
    Monitors,
    Options,
}

pub struct App {
    pub monitors: Vec<Monitor>,
    pub configs: Vec<MonitorConfig>,
    pub monitor_list_state: ListState,
    pub option_list_state: ListState,
    pub focused_pane: FocusedPane,
    pub info_message: Option<String>,
}

impl App {
    pub fn new() -> io::Result<Self> {
        let monitors_data = commands::fetch_monitors()?;
        let (monitors, configs) = Self::parse_monitors(monitors_data)?;
        let monitor_count = monitors.len();

        Ok(Self {
            monitors,
            configs,
            monitor_list_state: Self::init_list_state(monitor_count),
            option_list_state: Self::init_list_state(OPTION_COUNT),
            focused_pane: FocusedPane::Monitors,
            info_message: None,
        })
    }

    fn init_list_state(count: usize) -> ListState {
        let mut state = ListState::default();
        if count > 0 {
            state.select(Some(0));
        }
        state
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
                dpms_on: true,
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

    fn get_other_monitor_info(&self, current_idx: usize) -> Option<(usize, String)> {
        self.monitors
            .iter()
            .enumerate()
            .find(|(i, m)| *i != current_idx && m.active)
            .map(|(i, m)| (i, m.name.clone()))
    }

    fn set_as_main(&self) {
        let Some(idx) = self.monitor_list_state.selected() else {
            return;
        };
        let monitor = &self.monitors[idx];
        let config = &self.configs[idx];

        let command = format!(
            "hyprctl keyword monitor \"{},preferred,{}@{:.2},auto,{:.2}\"",
            monitor.name,
            config.resolution,
            config.refresh_rate,
            config.scale_as_float()
        );
        commands::execute_hyprctl(&command);
    }

    fn extend_relative(&self, direction: &str) {
        let Some(idx) = self.monitor_list_state.selected() else {
            return;
        };

        if let Some((_, other_monitor_name)) = self.get_other_monitor_info(idx) {
            let monitor = &self.monitors[idx];
            let config = &self.configs[idx];

            let command = format!(
                "hyprctl keyword monitor \"{},{}@{:.2},auto,{:.2},{}of,{}\"",
                monitor.name,
                config.resolution,
                config.refresh_rate,
                config.scale_as_float(),
                direction,
                other_monitor_name
            );
            commands::execute_hyprctl(&command);
        }
    }

    fn mirror_monitor(&mut self) {
        let Some(idx) = self.monitor_list_state.selected() else {
            return;
        };

        if let Some((other_idx, other_monitor_name)) = self.get_other_monitor_info(idx) {
            let source_scale;
            let source_resolution;
            let source_refresh_rate;
            let source_scale_float;

            {
                let source_config = &self.configs[idx];
                source_scale = source_config.scale;
                source_resolution = source_config.resolution.clone();
                source_refresh_rate = source_config.refresh_rate;
                source_scale_float = source_config.scale_as_float();
            }

            let source_monitor_name = &self.monitors[idx].name;

            self.configs[other_idx].scale = source_scale;

            let command = format!(
                "hyprctl keyword monitor \"{},{}@{:.2},auto,{:.2},mirror,{}\"",
                source_monitor_name,
                source_resolution,
                source_refresh_rate,
                source_scale_float,
                other_monitor_name
            );
            commands::execute_hyprctl(&command);
        }
    }

    fn toggle_dpms(&mut self) {
        let Some(idx) = self.monitor_list_state.selected() else {
            return;
        };

        let is_on = self.configs[idx].dpms_on;
        let monitor_name = &self.monitors[idx].name;

        let command = if is_on {
            format!("hyprctl dispatch dpms off {}", monitor_name)
        } else {
            format!("hyprctl dispatch dpms on {}", monitor_name)
        };

        if commands::execute_hyprctl(&command) {
            self.configs[idx].dpms_on = !is_on;
        }
    }

    fn apply_changes(&self) {
        let Some(idx) = self.monitor_list_state.selected() else {
            return;
        };

        let monitor = &self.monitors[idx];
        let config = &self.configs[idx];

        let command = format!(
            "hyprctl keyword monitor \"{},{}@{:.2},auto,{:.2}\"",
            monitor.name,
            config.resolution,
            config.refresh_rate,
            config.scale_as_float()
        );

        commands::execute_hyprctl(&command);
    }

    fn save_config_to_file(&mut self) {
        let path_str = "~/.config/hypr/monitors.conf";
        let expanded_path = match shellexpand::full(path_str) {
            Ok(p) => PathBuf::from(p.into_owned()),
            Err(e) => {
                self.info_message = Some(format!("Error expanding path: {}", e));
                return;
            }
        };

        if let Some(parent) = expanded_path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                self.info_message = Some(format!("Error creating dir: {}", e));
                return;
            }
        }

        let mut file_content = String::from("# Monitor settings generated by hypr-tui\n# Add 'source = ~/.config/hypr/monitors.conf' to your hyprland.conf\n\n");

        for (i, monitor) in self.monitors.iter().enumerate() {
            if monitor.active {
                let config = &self.configs[i];
                let line = format!(
                    "monitor={},{}@{:.2},auto,{:.2}\n",
                    monitor.name,
                    config.resolution,
                    config.refresh_rate,
                    config.scale_as_float()
                );
                file_content.push_str(&line);
            }
        }

        match fs::write(&expanded_path, file_content) {
            Ok(_) => {
                self.info_message = Some(format!("Success! Saved to {}", expanded_path.display()))
            }
            Err(e) => self.info_message = Some(format!("Error writing file: {}", e)),
        }
    }

    fn disable_monitor(&self) {
        let Some(idx) = self.monitor_list_state.selected() else {
            return;
        };

        let command = format!(
            "hyprctl keyword monitor \"{},disable\"",
            self.monitors[idx].name
        );

        commands::execute_hyprctl(&command);
    }

    fn toggle_pane(&mut self) {
        self.focused_pane = if self.focused_pane == FocusedPane::Monitors {
            FocusedPane::Options
        } else {
            FocusedPane::Monitors
        };
    }

    pub fn handle_key(&mut self, code: KeyCode) -> bool {
        if self.info_message.is_some() {
            self.info_message = None;
        }

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
                    Some(SET_MAIN_IDX) => self.set_as_main(),
                    Some(EXTEND_LEFT_IDX) => self.extend_relative("left"),
                    Some(EXTEND_RIGHT_IDX) => self.extend_relative("right"),
                    Some(MIRROR_IDX) => self.mirror_monitor(),
                    Some(BLACK_SCREEN_IDX) => self.toggle_dpms(),
                    Some(SAVE_OPTION_IDX) => self.save_config_to_file(),
                    Some(DISABLE_OPTION_IDX) => self.disable_monitor(),
                    _ => {}
                }
            }
            _ => {}
        }
        false
    }

    pub fn selected_monitor(&self) -> Option<usize> {
        self.monitor_list_state.selected()
    }

    pub fn is_focused(&self, pane: FocusedPane) -> bool {
        self.focused_pane == pane
    }
}
