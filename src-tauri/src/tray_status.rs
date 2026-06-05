use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration as StdDuration;

use tauri::menu::{
    CheckMenuItem, CheckMenuItemBuilder, Menu, MenuBuilder, MenuEvent, MenuItemBuilder,
};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    App, AppHandle, LogicalPosition, Manager, Monitor, PhysicalPosition, Position, Runtime,
    WebviewUrl, WebviewWindow, WebviewWindowBuilder, Window, WindowEvent, Wry,
};
use tokio::time::sleep;

use crate::db::{TokenPulseSnapshot, TokenPulseWindowPosition, TokenScopeRepository};
use crate::AppState;

const TOKEN_PULSE_TRAY_ID: &str = "token-pulse-tray";
const MAIN_WINDOW_LABEL: &str = "main";
const MAIN_WINDOW_TITLE: &str = "TokenScope Desktop";
const MAIN_WINDOW_WIDTH: f64 = 1180.0;
const MAIN_WINDOW_HEIGHT: f64 = 760.0;
const MAIN_WINDOW_MIN_WIDTH: f64 = 960.0;
const MAIN_WINDOW_MIN_HEIGHT: f64 = 640.0;
const TOKEN_PULSE_WINDOW_LABEL: &str = "token-pulse";
const TOKEN_PULSE_DETAIL_WINDOW_LABEL: &str = "token-pulse-detail";
const TOKEN_PULSE_POSITION_X_SETTING: &str = "token_pulse_window_x";
const TOKEN_PULSE_POSITION_Y_SETTING: &str = "token_pulse_window_y";
const TOKEN_PULSE_LOCKED_SETTING: &str = "token_pulse_position_locked";
const TOKEN_PULSE_VISIBLE_SETTING: &str = "token_pulse_visible";
const TOKEN_PULSE_MENU_OPEN_MAIN: &str = "token-pulse-open-main";
const TOKEN_PULSE_MENU_TOGGLE_VISIBLE: &str = "token-pulse-toggle-visible";
const TOKEN_PULSE_MENU_EXIT: &str = "token-pulse-exit";
const TOKEN_PULSE_CONTEXT_OPEN_MAIN: &str = "token-pulse-context-open-main";
const TOKEN_PULSE_CONTEXT_HIDE: &str = "token-pulse-context-hide";
const TOKEN_PULSE_CONTEXT_LOCK_POSITION: &str = "token-pulse-context-lock-position";
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
const TOKEN_PULSE_FULLSCREEN_GUARD_SECONDS: u64 = 2;

#[derive(Debug, Default)]
struct TokenPulseInteractionState {
    mini_hovered: bool,
    detail_hovered: bool,
    hide_for_fullscreen: bool,
    was_visible_before_fullscreen: bool,
}

impl TokenPulseInteractionState {
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

static TOKEN_PULSE_HOVER_STATE: OnceLock<Mutex<TokenPulseInteractionState>> = OnceLock::new();
static TOKEN_PULSE_DRAG_SAVE_GENERATION: AtomicU64 = AtomicU64::new(0);

struct TokenPulseContextMenu {
    menu: Menu<Wry>,
    lock_item: CheckMenuItem<Wry>,
}

fn token_pulse_hover_state() -> &'static Mutex<TokenPulseInteractionState> {
    TOKEN_PULSE_HOVER_STATE.get_or_init(|| Mutex::new(TokenPulseInteractionState::default()))
}

pub fn setup_token_pulse_tray(
    app: &App,
    repository: TokenScopeRepository,
) -> tauri::Result<()> {
    let Some(icon) = app.default_window_icon().cloned() else {
        return Ok(());
    };

    let app_handle = app.handle().clone();
    let event_app_handle = app_handle.clone();
    let tray_menu = build_token_pulse_tray_menu(app)?;
    if app.try_state::<TokenPulseContextMenu>().is_none() {
        app.manage(build_token_pulse_context_menu(app)?);
    }
    let tray = TrayIconBuilder::with_id(TOKEN_PULSE_TRAY_ID)
        .icon(icon)
        .tooltip("TokenScope Desktop\nToday Token: loading...")
        .menu(&tray_menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(move |_tray, event| handle_tray_icon_event(&event_app_handle, event))
        .build(app)?;

    spawn_token_pulse_tray_updater(tray, repository.clone());
    setup_token_pulse_window(app, repository.clone())?;
    setup_token_pulse_detail_window(app)?;
    spawn_token_pulse_fullscreen_guard(app_handle);
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

    if token_pulse_position_locked(&app) {
        return Ok(());
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

#[tauri::command]
pub fn show_token_pulse_context_menu(app: AppHandle, window: Window) -> Result<(), String> {
    if window.label() != TOKEN_PULSE_WINDOW_LABEL {
        return Err(
            "token pulse context menu can only run from the token pulse window".to_string(),
        );
    }

    let context_menu = app
        .try_state::<TokenPulseContextMenu>()
        .ok_or_else(|| "token pulse context menu was not initialized".to_string())?;
    context_menu
        .lock_item
        .set_checked(token_pulse_position_locked(&app))
        .map_err(|err| err.to_string())?;

    window
        .popup_menu(&context_menu.menu)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn get_token_pulse_position_locked(app: AppHandle) -> bool {
    token_pulse_position_locked(&app)
}

#[tauri::command]
pub fn set_token_pulse_position_locked(app: AppHandle, locked: bool) -> Result<(), String> {
    set_token_pulse_position_locked_state(&app, locked)
}

fn setup_token_pulse_window<R: Runtime>(
    app: &App<R>,
    repository: TokenScopeRepository,
) -> tauri::Result<()> {
    if app.get_webview_window(TOKEN_PULSE_WINDOW_LABEL).is_some() {
        return Ok(());
    }

    let (x, y) = token_pulse_window_position(app, &repository)?;
    let initially_visible = stored_token_pulse_visible(&repository);
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
    .visible(initially_visible)
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

fn hide_token_pulse_detail_window<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
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

fn stored_token_pulse_visible(repository: &TokenScopeRepository) -> bool {
    debug_assert_eq!(TOKEN_PULSE_VISIBLE_SETTING, "token_pulse_visible");

    tauri::async_runtime::block_on(repository.token_pulse_visible()).unwrap_or(true)
}

fn token_pulse_position_locked<R: Runtime>(app: &AppHandle<R>) -> bool {
    debug_assert_eq!(TOKEN_PULSE_LOCKED_SETTING, "token_pulse_position_locked");

    tauri::async_runtime::block_on(
        app.state::<AppState>()
            .repository
            .token_pulse_position_locked(),
    )
    .unwrap_or(false)
}

fn set_token_pulse_position_locked_state<R: Runtime>(
    app: &AppHandle<R>,
    locked: bool,
) -> Result<(), String> {
    tauri::async_runtime::block_on(
        app.state::<AppState>()
            .repository
            .save_token_pulse_position_locked(locked),
    )
    .map_err(|err| err.to_string())
}

fn token_pulse_user_visible<R: Runtime>(app: &AppHandle<R>) -> bool {
    tauri::async_runtime::block_on(app.state::<AppState>().repository.token_pulse_visible())
        .unwrap_or(true)
}

fn token_pulse_window_currently_visible<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.get_webview_window(TOKEN_PULSE_WINDOW_LABEL)
        .and_then(|window| window.is_visible().ok())
        .unwrap_or_else(|| token_pulse_user_visible(app))
}

fn set_token_pulse_user_visible<R: Runtime>(
    app: &AppHandle<R>,
    visible: bool,
) -> Result<(), String> {
    tauri::async_runtime::block_on(
        app.state::<AppState>()
            .repository
            .save_token_pulse_visible(visible),
    )
    .map_err(|err| err.to_string())
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

fn build_token_pulse_context_menu<M: Manager<Wry>>(
    manager: &M,
) -> tauri::Result<TokenPulseContextMenu> {
    let open_main =
        MenuItemBuilder::with_id(TOKEN_PULSE_CONTEXT_OPEN_MAIN, "打开主界面").build(manager)?;
    let hide = MenuItemBuilder::with_id(TOKEN_PULSE_CONTEXT_HIDE, "隐藏常驻小窗").build(manager)?;
    let lock_item =
        CheckMenuItemBuilder::with_id(TOKEN_PULSE_CONTEXT_LOCK_POSITION, "锁定当前小窗位置")
            .checked(token_pulse_position_locked(manager.app_handle()))
            .build(manager)?;
    let menu = MenuBuilder::new(manager)
        .item(&open_main)
        .separator()
        .item(&hide)
        .item(&lock_item)
        .build()?;

    Ok(TokenPulseContextMenu { menu, lock_item })
}

fn build_token_pulse_tray_menu<R: Runtime, M: Manager<R>>(
    manager: &M,
) -> tauri::Result<tauri::menu::Menu<R>> {
    let open_main =
        MenuItemBuilder::with_id(TOKEN_PULSE_MENU_OPEN_MAIN, "打开主界面").build(manager)?;
    let toggle_visible =
        CheckMenuItemBuilder::with_id(TOKEN_PULSE_MENU_TOGGLE_VISIBLE, "显示常驻小窗")
            .checked(token_pulse_window_currently_visible(manager.app_handle()))
            .build(manager)?;
    let exit = MenuItemBuilder::with_id(TOKEN_PULSE_MENU_EXIT, "退出").build(manager)?;

    MenuBuilder::new(manager)
        .item(&open_main)
        .separator()
        .item(&toggle_visible)
        .separator()
        .item(&exit)
        .build()
}

pub fn handle_token_pulse_menu_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    #[cfg(debug_assertions)]
    eprintln!("[tokenscope] menu event: {}", event.id().as_ref());

    match event.id().as_ref() {
        TOKEN_PULSE_MENU_OPEN_MAIN | TOKEN_PULSE_CONTEXT_OPEN_MAIN => show_main_window(app),
        TOKEN_PULSE_MENU_TOGGLE_VISIBLE => toggle_token_pulse_window(app),
        TOKEN_PULSE_CONTEXT_HIDE => hide_token_pulse_window(app),
        TOKEN_PULSE_CONTEXT_LOCK_POSITION => {
            let locked = !token_pulse_position_locked(app);
            let _ = set_token_pulse_position_locked_state(app, locked);
        }
        TOKEN_PULSE_MENU_EXIT => exit_token_scope(app),
        _ => {}
    }
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

fn toggle_token_pulse_window<R: Runtime>(app: &AppHandle<R>) {
    let visible = !token_pulse_window_currently_visible(app);
    set_token_pulse_window_visible(app, visible);
}

fn set_token_pulse_window_visible<R: Runtime>(app: &AppHandle<R>, visible: bool) {
    #[cfg(debug_assertions)]
    eprintln!("[tokenscope] set token pulse visible: {visible}");

    let _ = set_token_pulse_user_visible(app, visible);
    set_token_pulse_tray_toggle_checked(app, visible);

    if visible {
        if is_probably_fullscreen() {
            #[cfg(debug_assertions)]
            eprintln!("[tokenscope] token pulse hidden because foreground window is fullscreen");

            let _ = hide_token_pulse_detail_window(app);
            return;
        }
        if let Some(window) = app.get_webview_window(TOKEN_PULSE_WINDOW_LABEL) {
            let _ = window.show();
        } else {
            #[cfg(debug_assertions)]
            eprintln!("[tokenscope] token pulse window was not found");
        }
    } else {
        let _ = hide_token_pulse_detail_window(app);
        if let Some(window) = app.get_webview_window(TOKEN_PULSE_WINDOW_LABEL) {
            let _ = window.hide();
        }
    }
}

fn set_token_pulse_tray_toggle_checked<R: Runtime>(app: &AppHandle<R>, visible: bool) {
    let Some(tray) = app.tray_by_id(TOKEN_PULSE_TRAY_ID) else {
        return;
    };
    let Ok(tray_menu) = build_token_pulse_tray_menu(app) else {
        return;
    };

    if let Some(item) = tray_menu.get(TOKEN_PULSE_MENU_TOGGLE_VISIBLE) {
        if let Some(check_item) = item.as_check_menuitem() {
            let _ = check_item.set_checked(visible);
        }
    }

    let _ = tray.set_menu(Some(tray_menu));
}

fn hide_token_pulse_window<R: Runtime>(app: &AppHandle<R>) {
    set_token_pulse_window_visible(app, false);
}

fn exit_token_scope<R: Runtime>(app: &AppHandle<R>) {
    app.exit(0);
}

fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    let window = find_main_window(app).or_else(|| {
        create_main_window(app)
            .map_err(|err| {
                #[cfg(debug_assertions)]
                eprintln!("[tokenscope] failed to create main window: {err}");
                err
            })
            .ok()
    });

    if let Some(window) = window {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        #[cfg(debug_assertions)]
        eprintln!(
            "[tokenscope] main window was not found; windows: {}",
            debug_webview_window_labels(app)
        );
    }
}

fn find_main_window<R: Runtime>(app: &AppHandle<R>) -> Option<WebviewWindow<R>> {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        return Some(window);
    }

    app.webview_windows().into_values().find(|window| {
        let title = window.title().unwrap_or_default();
        title == MAIN_WINDOW_TITLE
    })
}

fn create_main_window<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<WebviewWindow<R>> {
    WebviewWindowBuilder::new(
        app,
        MAIN_WINDOW_LABEL,
        WebviewUrl::App("index.html".into()),
    )
    .title(MAIN_WINDOW_TITLE)
    .inner_size(MAIN_WINDOW_WIDTH, MAIN_WINDOW_HEIGHT)
    .min_inner_size(MAIN_WINDOW_MIN_WIDTH, MAIN_WINDOW_MIN_HEIGHT)
    .build()
}

#[cfg(debug_assertions)]
fn debug_webview_window_labels<R: Runtime>(app: &AppHandle<R>) -> String {
    app.webview_windows()
        .into_iter()
        .map(|(label, window)| {
            let title = window.title().unwrap_or_else(|_| "<unknown>".to_string());
            format!("{label}={title}")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn spawn_token_pulse_fullscreen_guard<R: Runtime>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        loop {
            hide_for_fullscreen(&app, is_probably_fullscreen());
            sleep(StdDuration::from_secs(TOKEN_PULSE_FULLSCREEN_GUARD_SECONDS)).await;
        }
    });
}

fn hide_for_fullscreen<R: Runtime>(app: &AppHandle<R>, fullscreen: bool) {
    let Some(window) = app.get_webview_window(TOKEN_PULSE_WINDOW_LABEL) else {
        return;
    };

    if fullscreen {
        let mut state = match token_pulse_hover_state().lock() {
            Ok(state) => state,
            Err(_) => return,
        };
        if state.hide_for_fullscreen {
            return;
        }

        state.was_visible_before_fullscreen = window.is_visible().unwrap_or(false);
        state.hide_for_fullscreen = true;
        drop(state);

        let _ = hide_token_pulse_detail_window(app);
        let _ = window.hide();
        return;
    }

    let should_restore = {
        let mut state = match token_pulse_hover_state().lock() {
            Ok(state) => state,
            Err(_) => return,
        };
        if !state.hide_for_fullscreen {
            return;
        }

        let should_restore = state.was_visible_before_fullscreen && token_pulse_user_visible(app);
        state.hide_for_fullscreen = false;
        state.was_visible_before_fullscreen = false;
        should_restore
    };

    if should_restore {
        let _ = window.show();
    }
}

#[cfg(target_os = "windows")]
fn is_probably_fullscreen() -> bool {
    windows_foreground_window_is_fullscreen()
}

#[cfg(not(target_os = "windows"))]
fn is_probably_fullscreen() -> bool {
    false
}

#[cfg(target_os = "windows")]
fn windows_foreground_window_is_fullscreen() -> bool {
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::Graphics::Dwm::{
        DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS,
    };
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowRect, IsIconic, IsWindowVisible,
    };

    unsafe {
        let hwnd: HWND = GetForegroundWindow();
        if hwnd.is_null() || IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 {
            return false;
        }

        let mut rect = RECT::default();
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            &mut rect as *mut _ as *mut _,
            std::mem::size_of::<RECT>() as u32,
        ) != 0
            && GetWindowRect(hwnd, &mut rect) == 0
        {
            return false;
        }

        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        if monitor.is_null() {
            return false;
        }

        let mut monitor_info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            rcMonitor: RECT::default(),
            rcWork: RECT::default(),
            dwFlags: 0,
        };
        if GetMonitorInfoW(monitor, &mut monitor_info) == 0 {
            return false;
        }

        rect_covers_monitor_area(
            DesktopRect::from(rect),
            DesktopRect::from(monitor_info.rcMonitor),
            DesktopRect::from(monitor_info.rcWork),
        )
    }
}

#[cfg(target_os = "windows")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DesktopRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[cfg(target_os = "windows")]
impl From<windows_sys::Win32::Foundation::RECT> for DesktopRect {
    fn from(rect: windows_sys::Win32::Foundation::RECT) -> Self {
        Self {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        }
    }
}

#[cfg(target_os = "windows")]
fn rect_covers_monitor_area(rect: DesktopRect, monitor: DesktopRect, work_area: DesktopRect) -> bool {
    const FULLSCREEN_TOLERANCE_PX: i32 = 2;

    if !rect_matches(monitor, work_area, FULLSCREEN_TOLERANCE_PX)
        && rect_matches(rect, work_area, FULLSCREEN_TOLERANCE_PX)
    {
        return false;
    }

    rect.left <= monitor.left + FULLSCREEN_TOLERANCE_PX
        && rect.top <= monitor.top + FULLSCREEN_TOLERANCE_PX
        && rect.right >= monitor.right - FULLSCREEN_TOLERANCE_PX
        && rect.bottom >= monitor.bottom - FULLSCREEN_TOLERANCE_PX
}

#[cfg(target_os = "windows")]
fn rect_matches(left: DesktopRect, right: DesktopRect, tolerance: i32) -> bool {
    (left.left - right.left).abs() <= tolerance
        && (left.top - right.top).abs() <= tolerance
        && (left.right - right.right).abs() <= tolerance
        && (left.bottom - right.bottom).abs() <= tolerance
}

#[cfg(all(test, target_os = "windows"))]
mod fullscreen_detection_tests {
    use super::*;

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> DesktopRect {
        DesktopRect {
            left,
            top,
            right,
            bottom,
        }
    }

    #[test]
    fn work_area_sized_window_is_not_fullscreen() {
        let monitor = rect(0, 0, 1920, 1080);
        let work_area = rect(0, 0, 1920, 1040);

        assert!(!rect_covers_monitor_area(work_area, monitor, work_area));
    }

    #[test]
    fn monitor_sized_window_is_fullscreen() {
        let monitor = rect(0, 0, 1920, 1080);
        let work_area = rect(0, 0, 1920, 1040);

        assert!(rect_covers_monitor_area(monitor, monitor, work_area));
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
