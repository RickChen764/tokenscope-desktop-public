use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration as StdDuration;

use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    App, AppHandle, LogicalPosition, Manager, Monitor, PhysicalPosition, Position, Runtime,
    WebviewUrl, WebviewWindow, WebviewWindowBuilder, Window, WindowEvent,
};
use tokio::time::sleep;

use crate::db::{TokenPulseSnapshot, TokenPulseWindowPosition, TokenScopeRepository};

const TOKEN_PULSE_TRAY_ID: &str = "token-pulse-tray";
const TOKEN_PULSE_WINDOW_LABEL: &str = "token-pulse";
const TOKEN_PULSE_DETAIL_WINDOW_LABEL: &str = "token-pulse-detail";
const TOKEN_PULSE_POSITION_X_SETTING: &str = "token_pulse_window_x";
const TOKEN_PULSE_POSITION_Y_SETTING: &str = "token_pulse_window_y";
const TOKEN_PULSE_COLLAPSED_WIDTH: f64 = 360.0;
const TOKEN_PULSE_COLLAPSED_HEIGHT: f64 = 56.0;
const TOKEN_PULSE_DETAIL_WIDTH: f64 = 360.0;
const TOKEN_PULSE_DETAIL_HEIGHT: f64 = 364.0;
const TOKEN_PULSE_WINDOW_MARGIN: f64 = 16.0;
const TOKEN_PULSE_DETAIL_GAP: f64 = 8.0;
const TOKEN_PULSE_HISTORY_DAYS: i64 = 30;
const TOKEN_PULSE_REFRESH_SECONDS: u64 = 60;
const TOKEN_PULSE_DETAIL_HIDE_DELAY_MS: u64 = 180;
const TOKEN_PULSE_DRAG_DEBOUNCE_MS: u64 = 300;

#[derive(Debug, Default)]
struct TokenPulseHoverState {
    mini_hovered: bool,
    detail_hovered: bool,
}

impl TokenPulseHoverState {
    fn is_hovered(&self) -> bool {
        self.mini_hovered || self.detail_hovered
    }
}

#[derive(Debug, Clone, Copy)]
struct LogicalWorkArea {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

static TOKEN_PULSE_HOVER_STATE: OnceLock<Mutex<TokenPulseHoverState>> = OnceLock::new();
static TOKEN_PULSE_DRAG_SAVE_GENERATION: AtomicU64 = AtomicU64::new(0);

fn token_pulse_hover_state() -> &'static Mutex<TokenPulseHoverState> {
    TOKEN_PULSE_HOVER_STATE.get_or_init(|| Mutex::new(TokenPulseHoverState::default()))
}

pub fn setup_token_pulse_tray<R: Runtime>(
    app: &App<R>,
    repository: TokenScopeRepository,
) -> tauri::Result<()> {
    let Some(icon) = app.default_window_icon().cloned() else {
        return Ok(());
    };

    let app_handle = app.handle().clone();
    let event_app_handle = app_handle.clone();
    let tray = TrayIconBuilder::with_id(TOKEN_PULSE_TRAY_ID)
        .icon(icon)
        .tooltip("TokenScope Desktop\nToday Token: loading...")
        .show_menu_on_left_click(false)
        .on_tray_icon_event(move |_tray, event| handle_tray_icon_event(&event_app_handle, event))
        .build(app)?;

    spawn_token_pulse_tray_updater(tray, repository.clone());
    setup_token_pulse_window(app, repository.clone())?;
    setup_token_pulse_detail_window(app)?;
    Ok(())
}

#[tauri::command]
pub fn set_token_pulse_detail_hovered(
    app: AppHandle,
    window: Window,
    source: String,
    hovered: bool,
) -> Result<(), String> {
    if window.label() != TOKEN_PULSE_WINDOW_LABEL
        && window.label() != TOKEN_PULSE_DETAIL_WINDOW_LABEL
    {
        return Err("token pulse hover can only run from token pulse windows".to_string());
    }

    {
        let mut state = token_pulse_hover_state()
            .lock()
            .map_err(|_| "token pulse hover state was poisoned".to_string())?;
        match source.as_str() {
            "mini" => state.mini_hovered = hovered,
            "detail" => state.detail_hovered = hovered,
            _ => return Err(format!("unsupported token pulse hover source: {source}")),
        }
    }

    if hovered && source == "mini" {
        show_token_pulse_detail_window(&app, &window)?;
    } else {
        let app_handle = app.clone();
        tauri::async_runtime::spawn(async move {
            sleep(StdDuration::from_millis(TOKEN_PULSE_DETAIL_HIDE_DELAY_MS)).await;
            let should_hide = token_pulse_hover_state()
                .lock()
                .map(|state| !state.is_hovered())
                .unwrap_or(true);

            if should_hide {
                let _ = hide_token_pulse_detail_window(&app_handle);
            }
        });
    }

    Ok(())
}

#[tauri::command]
pub fn set_token_pulse_dragging(
    app: AppHandle,
    window: Window,
    dragging: bool,
) -> Result<(), String> {
    if window.label() != TOKEN_PULSE_WINDOW_LABEL {
        return Err("token pulse dragging can only run from the token pulse window".to_string());
    }

    if dragging {
        {
            let mut state = token_pulse_hover_state()
                .lock()
                .map_err(|_| "token pulse hover state was poisoned".to_string())?;
            state.mini_hovered = false;
            state.detail_hovered = false;
        }
        let _ = hide_token_pulse_detail_window(&app);
    }

    Ok(())
}

fn setup_token_pulse_window<R: Runtime>(
    app: &App<R>,
    repository: TokenScopeRepository,
) -> tauri::Result<()> {
    if app.get_webview_window(TOKEN_PULSE_WINDOW_LABEL).is_some() {
        return Ok(());
    }

    let (x, y) = token_pulse_window_position(app, &repository)?;
    let window = WebviewWindowBuilder::new(
        app,
        TOKEN_PULSE_WINDOW_LABEL,
        WebviewUrl::App("index.html?tokenPulse=1".into()),
    )
    .title("TokenScope Token Pulse")
    .inner_size(TOKEN_PULSE_COLLAPSED_WIDTH, TOKEN_PULSE_COLLAPSED_HEIGHT)
    .position(x, y)
    .decorations(false)
    .resizable(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .shadow(true)
    .visible(true)
    .build()?;
    track_token_pulse_window_position(window, repository);

    Ok(())
}

fn track_token_pulse_window_position<R: Runtime>(
    window: WebviewWindow<R>,
    repository: TokenScopeRepository,
) {
    let tracked_window = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Moved(position) = event {
            if let Some(position) =
                token_pulse_position_from_window_event(&tracked_window, position)
            {
                schedule_save_token_pulse_window_position(repository.clone(), position);
            }
        }
    });
}

fn token_pulse_position_from_window_event<R: Runtime>(
    window: &WebviewWindow<R>,
    position: &PhysicalPosition<i32>,
) -> Option<TokenPulseWindowPosition> {
    let scale_factor = window.scale_factor().ok()?;
    let x = position.x as f64 / scale_factor;
    let y = position.y as f64 / scale_factor;

    Some(
        window
            .current_monitor()
            .ok()
            .flatten()
            .map(|monitor| {
                let (x, y) = clamp_token_pulse_position(
                    logical_work_area(&monitor),
                    x,
                    y,
                    TOKEN_PULSE_COLLAPSED_WIDTH,
                    TOKEN_PULSE_COLLAPSED_HEIGHT,
                );
                TokenPulseWindowPosition { x, y }
            })
            .unwrap_or(TokenPulseWindowPosition { x, y }),
    )
}

fn schedule_save_token_pulse_window_position(
    repository: TokenScopeRepository,
    position: TokenPulseWindowPosition,
) {
    let generation = TOKEN_PULSE_DRAG_SAVE_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    tauri::async_runtime::spawn(async move {
        sleep(StdDuration::from_millis(TOKEN_PULSE_DRAG_DEBOUNCE_MS)).await;
        if TOKEN_PULSE_DRAG_SAVE_GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }

        save_token_pulse_window_position(&repository, position).await;
    });
}

async fn save_token_pulse_window_position(
    repository: &TokenScopeRepository,
    position: TokenPulseWindowPosition,
) {
    let _ = repository.save_token_pulse_window_position(position).await;
}

fn setup_token_pulse_detail_window<R: Runtime>(app: &App<R>) -> tauri::Result<()> {
    if app
        .get_webview_window(TOKEN_PULSE_DETAIL_WINDOW_LABEL)
        .is_some()
    {
        return Ok(());
    }

    let (x, y) = token_pulse_detail_window_position(app)?;
    WebviewWindowBuilder::new(
        app,
        TOKEN_PULSE_DETAIL_WINDOW_LABEL,
        WebviewUrl::App("index.html?tokenPulseDetail=1".into()),
    )
    .title("TokenScope Token Pulse Detail")
    .inner_size(TOKEN_PULSE_DETAIL_WIDTH, TOKEN_PULSE_DETAIL_HEIGHT)
    .position(x, y)
    .decorations(false)
    .resizable(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .shadow(true)
    .visible(false)
    .build()?;

    Ok(())
}

fn show_token_pulse_detail_window(app: &AppHandle, anchor_window: &Window) -> Result<(), String> {
    let detail_window = app
        .get_webview_window(TOKEN_PULSE_DETAIL_WINDOW_LABEL)
        .ok_or_else(|| "token pulse detail window was not found".to_string())?;
    let monitor = anchor_window
        .current_monitor()
        .map_err(|err| err.to_string())?
        .or_else(|| app.primary_monitor().ok().flatten())
        .ok_or_else(|| "token pulse monitor was not found".to_string())?;
    let anchor_position = anchor_window
        .outer_position()
        .map_err(|err| err.to_string())?;
    let scale_factor = anchor_window
        .scale_factor()
        .map_err(|err| err.to_string())?;
    let anchor_x = anchor_position.x as f64 / scale_factor;
    let anchor_y = anchor_position.y as f64 / scale_factor;
    let (x, y) = token_pulse_detail_window_position_for_anchor(&monitor, anchor_x, anchor_y);

    detail_window
        .set_position(Position::Logical(LogicalPosition::new(
            x.round(),
            y.round(),
        )))
        .map_err(|err| err.to_string())?;
    detail_window.show().map_err(|err| err.to_string())?;
    Ok(())
}

fn hide_token_pulse_detail_window(app: &AppHandle) -> Result<(), String> {
    let detail_window = app
        .get_webview_window(TOKEN_PULSE_DETAIL_WINDOW_LABEL)
        .ok_or_else(|| "token pulse detail window was not found".to_string())?;
    detail_window.hide().map_err(|err| err.to_string())
}

fn token_pulse_window_position<R: Runtime>(
    app: &App<R>,
    repository: &TokenScopeRepository,
) -> tauri::Result<(f64, f64)> {
    let Some(monitor) = app.primary_monitor()? else {
        return Ok((TOKEN_PULSE_WINDOW_MARGIN, TOKEN_PULSE_WINDOW_MARGIN));
    };

    let work_area = logical_work_area(&monitor);
    Ok(stored_token_pulse_window_position(repository)
        .map(|position| {
            clamp_token_pulse_position(
                work_area,
                position.x,
                position.y,
                TOKEN_PULSE_COLLAPSED_WIDTH,
                TOKEN_PULSE_COLLAPSED_HEIGHT,
            )
        })
        .unwrap_or_else(|| {
            token_pulse_window_position_for_monitor(
                &monitor,
                TOKEN_PULSE_COLLAPSED_WIDTH,
                TOKEN_PULSE_COLLAPSED_HEIGHT,
            )
        }))
}

fn token_pulse_detail_window_position<R: Runtime>(app: &App<R>) -> tauri::Result<(f64, f64)> {
    let Some(monitor) = app.primary_monitor()? else {
        return Ok((TOKEN_PULSE_WINDOW_MARGIN, TOKEN_PULSE_WINDOW_MARGIN));
    };

    Ok(token_pulse_detail_window_position_for_monitor(&monitor))
}

fn stored_token_pulse_window_position(
    repository: &TokenScopeRepository,
) -> Option<TokenPulseWindowPosition> {
    debug_assert_eq!(TOKEN_PULSE_POSITION_X_SETTING, "token_pulse_window_x");
    debug_assert_eq!(TOKEN_PULSE_POSITION_Y_SETTING, "token_pulse_window_y");

    tauri::async_runtime::block_on(repository.token_pulse_window_position())
        .ok()
        .flatten()
}

fn logical_work_area(monitor: &Monitor) -> LogicalWorkArea {
    let scale_factor = monitor.scale_factor();
    let work_area = monitor.work_area();

    LogicalWorkArea {
        x: work_area.position.x as f64 / scale_factor,
        y: work_area.position.y as f64 / scale_factor,
        width: work_area.size.width as f64 / scale_factor,
        height: work_area.size.height as f64 / scale_factor,
    }
}

fn clamp_token_pulse_position(
    work_area: LogicalWorkArea,
    x: f64,
    y: f64,
    window_width: f64,
    window_height: f64,
) -> (f64, f64) {
    (
        clamp_axis(x, work_area.x, work_area.x + work_area.width - window_width),
        clamp_axis(
            y,
            work_area.y,
            work_area.y + work_area.height - window_height,
        ),
    )
}

fn clamp_axis(value: f64, min: f64, max: f64) -> f64 {
    if max <= min {
        min
    } else {
        value.max(min).min(max)
    }
}

fn token_pulse_window_position_for_monitor(
    monitor: &Monitor,
    window_width: f64,
    window_height: f64,
) -> (f64, f64) {
    let work_area = logical_work_area(monitor);

    (
        work_area.x + work_area.width - window_width - TOKEN_PULSE_WINDOW_MARGIN,
        work_area.y + work_area.height - window_height - TOKEN_PULSE_WINDOW_MARGIN,
    )
}

fn token_pulse_detail_window_position_for_monitor(monitor: &Monitor) -> (f64, f64) {
    let work_area = logical_work_area(monitor);
    let mini_top =
        work_area.y + work_area.height - TOKEN_PULSE_COLLAPSED_HEIGHT - TOKEN_PULSE_WINDOW_MARGIN;

    (
        work_area.x + work_area.width - TOKEN_PULSE_DETAIL_WIDTH - TOKEN_PULSE_WINDOW_MARGIN,
        (mini_top - TOKEN_PULSE_DETAIL_HEIGHT - TOKEN_PULSE_DETAIL_GAP)
            .max(work_area.y + TOKEN_PULSE_WINDOW_MARGIN),
    )
}

fn token_pulse_detail_window_position_for_anchor(
    monitor: &Monitor,
    anchor_x: f64,
    anchor_y: f64,
) -> (f64, f64) {
    let work_area = logical_work_area(monitor);
    let preferred_y = anchor_y - TOKEN_PULSE_DETAIL_HEIGHT - TOKEN_PULSE_DETAIL_GAP;
    let fallback_y = anchor_y + TOKEN_PULSE_COLLAPSED_HEIGHT + TOKEN_PULSE_DETAIL_GAP;
    let detail_y = if preferred_y >= work_area.y + TOKEN_PULSE_WINDOW_MARGIN {
        preferred_y
    } else {
        fallback_y
    };

    clamp_token_pulse_position(
        work_area,
        anchor_x,
        detail_y,
        TOKEN_PULSE_DETAIL_WIDTH,
        TOKEN_PULSE_DETAIL_HEIGHT,
    )
}

fn spawn_token_pulse_tray_updater<R: Runtime>(
    tray: tauri::tray::TrayIcon<R>,
    repository: TokenScopeRepository,
) {
    tauri::async_runtime::spawn(async move {
        loop {
            if let Ok(snapshot) = repository
                .token_pulse_snapshot(TOKEN_PULSE_HISTORY_DAYS)
                .await
            {
                let _ = tray.set_tooltip(Some(format_token_pulse_tooltip(&snapshot)));
            }

            sleep(StdDuration::from_secs(TOKEN_PULSE_REFRESH_SECONDS)).await;
        }
    });
}

fn handle_tray_icon_event<R: Runtime>(app: &AppHandle<R>, event: TrayIconEvent) {
    match event {
        TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        }
        | TrayIconEvent::DoubleClick {
            button: MouseButton::Left,
            ..
        } => show_main_window(app),
        _ => {}
    }
}

fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn format_token_pulse_tooltip(snapshot: &TokenPulseSnapshot) -> String {
    let average_label = format_compact_tokens_f64(snapshot.average_daily_tokens);
    let ratio_label = snapshot
        .ratio_to_average
        .map(|ratio| format!("{:.0}%", ratio * 100.0))
        .unwrap_or_else(|| "no average".to_string());
    let remaining_label = if snapshot.remaining_to_average > 0 {
        format!(
            "{} to avg",
            format_compact_tokens_i64(snapshot.remaining_to_average)
        )
    } else {
        "above avg".to_string()
    };

    format!(
        "TokenScope Desktop\nToday Token: {}\n30d avg: {} ({})\nYesterday: {}\n{}",
        format_compact_tokens_i64(snapshot.today_tokens),
        average_label,
        ratio_label,
        format_compact_tokens_i64(snapshot.yesterday_tokens),
        remaining_label
    )
}

fn format_compact_tokens_i64(tokens: i64) -> String {
    format_compact_tokens_f64(tokens as f64)
}

fn format_compact_tokens_f64(tokens: f64) -> String {
    let abs_tokens = tokens.abs();
    if abs_tokens >= 1_000_000_000.0 {
        format!("{:.2}B", tokens / 1_000_000_000.0)
    } else if abs_tokens >= 1_000_000.0 {
        format!("{:.0}M", tokens / 1_000_000.0)
    } else if abs_tokens >= 1_000.0 {
        format!("{:.0}K", tokens / 1_000.0)
    } else {
        format!("{:.0}", tokens)
    }
}
