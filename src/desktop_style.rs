//! Material-inspired surfaces, typography and owner-drawn native push buttons.
//! Input, tab navigation and accessibility remain standard Windows controls.
use std::{mem::size_of, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::{Dwm::*, Gdi::*},
    UI::{
        Accessibility::*, Controls::*, HiDpi::*, Input::KeyboardAndMouse::*, Shell::*,
        WindowsAndMessaging::*,
    },
};

const fn rgb(r: u32, g: u32, b: u32) -> COLORREF {
    r | (g << 8) | (b << 16)
}
// Neutral surfaces from VS Code's Dark Modern theme, with monochrome actions
// and focus borders instead of the upstream blue accent.
// https://github.com/microsoft/vscode/blob/main/extensions/theme-defaults/themes/dark_modern.json
const BACKGROUND: COLORREF = rgb(24, 24, 24); // #181818
const SURFACE: COLORREF = rgb(31, 31, 31); // #1F1F1F
const FIELD: COLORREF = rgb(49, 49, 49); // #313131
const INK: COLORREF = rgb(204, 204, 204); // #CCCCCC
const MUTED: COLORREF = rgb(157, 157, 157); // #9D9D9D
const PRIMARY_FILL: COLORREF = rgb(60, 60, 60);
const PRIMARY_HOVER: COLORREF = rgb(72, 72, 72);
const PRIMARY_PRESSED: COLORREF = rgb(52, 52, 52);
const ON_PRIMARY: COLORREF = rgb(222, 222, 222);
const FOCUS: COLORREF = rgb(128, 128, 128);
const OUTLINE: COLORREF = rgb(60, 60, 60); // #3C3C3C
pub const WIDTH: i32 = 760;
pub const HEIGHT: i32 = 680;
pub const EXPANDED_HEIGHT: i32 = 824;

pub struct Style {
    pub dpi: i32,
    pub body: HFONT,
    pub small: HFONT,
    pub heading: HFONT,
    pub title: HFONT,
    background: HBRUSH,
    surface: HBRUSH,
    field: HBRUSH,
    high_contrast: bool,
}
impl Style {
    pub fn new() -> Self {
        unsafe {
            let dpi = GetDpiForSystem().max(96) as i32;
            let font = |pixels: i32, weight: i32| {
                CreateFontW(
                    -(pixels * dpi / 96),
                    0,
                    0,
                    0,
                    weight,
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET as u32,
                    OUT_DEFAULT_PRECIS as u32,
                    CLIP_DEFAULT_PRECIS as u32,
                    CLEARTYPE_QUALITY as u32,
                    DEFAULT_PITCH as u32,
                    super::wide("Segoe UI").as_ptr(),
                )
            };
            let mut contrast: HIGHCONTRASTW = std::mem::zeroed();
            contrast.cbSize = size_of::<HIGHCONTRASTW>() as u32;
            SystemParametersInfoW(
                SPI_GETHIGHCONTRAST,
                contrast.cbSize,
                &mut contrast as *mut _ as _,
                0,
            );
            let high_contrast = contrast.dwFlags & HCF_HIGHCONTRASTON != 0;
            Self {
                dpi,
                body: font(15, FW_NORMAL as i32),
                small: font(13, FW_NORMAL as i32),
                heading: font(18, FW_SEMIBOLD as i32),
                title: font(28, FW_SEMIBOLD as i32),
                background: CreateSolidBrush(if high_contrast {
                    GetSysColor(COLOR_WINDOW)
                } else {
                    BACKGROUND
                }),
                surface: CreateSolidBrush(if high_contrast {
                    GetSysColor(COLOR_WINDOW)
                } else {
                    SURFACE
                }),
                field: CreateSolidBrush(if high_contrast {
                    GetSysColor(COLOR_WINDOW)
                } else {
                    FIELD
                }),
                high_contrast,
            }
        }
    }
    pub fn px(&self, value: i32) -> i32 {
        value * self.dpi / 96
    }
    pub fn owner_draw(&self) -> bool {
        !self.high_contrast
    }
    pub fn title_bar(&self, hwnd: HWND) {
        if !self.high_contrast {
            unsafe {
                let dark: i32 = 1;
                // Unsupported attributes are harmless on older Windows builds.
                DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
                    &dark as *const _ as _,
                    size_of::<i32>() as u32,
                );
                DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_CAPTION_COLOR as u32,
                    &BACKGROUND as *const _ as _,
                    size_of::<COLORREF>() as u32,
                );
                DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_TEXT_COLOR as u32,
                    &INK as *const _ as _,
                    size_of::<COLORREF>() as u32,
                );
            }
        }
    }
    pub fn font(&self, hwnd: HWND, font: HFONT) {
        unsafe {
            SendMessageW(hwnd, WM_SETFONT, font as usize, 1);
        }
    }
    pub fn prepare_button(&self, hwnd: HWND) {
        unsafe {
            SetWindowSubclass(hwnd, Some(pointer_proc), 3, 0);
        }
        if !self.high_contrast {
            unsafe {
                SetWindowSubclass(hwnd, Some(button_proc), 1, 0);
            }
        }
    }
    pub fn prepare_checkbox(&self, hwnd: HWND) {
        unsafe {
            SetWindowSubclass(hwnd, Some(pointer_proc), 3, 0);
        }
        if !self.high_contrast {
            // Style lives in an Rc owned by WindowState until all child windows
            // are destroyed. The subclass only reads this stable allocation.
            unsafe {
                SetWindowSubclass(hwnd, Some(checkbox_proc), 2, self as *const Self as usize);
            }
        }
    }
    fn draw_checkbox(&self, hwnd: HWND, dc: HDC) {
        unsafe {
            let mut client = RECT::default();
            GetClientRect(hwnd, &mut client);
            FillRect(dc, &client, self.surface);
            let checked = SendMessageW(hwnd, BM_GETCHECK, 0, 0) == BST_CHECKED as isize;
            let enabled = IsWindowEnabled(hwnd) != 0;
            let size = self.px(16);
            let top = (client.bottom - size) / 2;
            self.rounded(
                dc,
                RECT {
                    left: 0,
                    top,
                    right: size,
                    bottom: top + size,
                },
                FIELD,
                if enabled { OUTLINE } else { MUTED },
                4,
            );
            if checked {
                let points = [
                    POINT {
                        x: self.px(3),
                        y: top + self.px(8),
                    },
                    POINT {
                        x: self.px(7),
                        y: top + self.px(12),
                    },
                    POINT {
                        x: self.px(13),
                        y: top + self.px(4),
                    },
                ];
                let pen = CreatePen(PS_SOLID, self.px(2), if enabled { INK } else { MUTED });
                let old = SelectObject(dc, pen);
                Polyline(dc, points.as_ptr(), points.len() as i32);
                SelectObject(dc, old);
                DeleteObject(pen);
            }
            let old_font = SelectObject(dc, self.body);
            SetBkMode(dc, TRANSPARENT as i32);
            SetTextColor(dc, if enabled { INK } else { MUTED });
            let mut text_rect = RECT {
                left: self.px(26),
                ..client
            };
            let text = super::wide(&super::App::text(hwnd));
            DrawTextW(
                dc,
                text.as_ptr(),
                -1,
                &mut text_rect,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );
            if GetFocus() == hwnd {
                DrawFocusRect(dc, &text_rect);
            }
            SelectObject(dc, old_font);
        }
    }
    pub fn color(&self, parent: HWND, hwnd: HWND, dc: HDC, edit: bool) -> LRESULT {
        unsafe {
            let mut rect = RECT::default();
            GetWindowRect(hwnd, &mut rect);
            let mut point = POINT {
                x: rect.left,
                y: rect.top,
            };
            ScreenToClient(parent, &mut point);
            let is_card = point.y >= self.px(100) && point.y < self.px(620);
            let (brush, color) = if self.high_contrast {
                (self.surface, GetSysColor(COLOR_WINDOW))
            } else if edit {
                (self.field, FIELD)
            } else if is_card {
                (self.surface, SURFACE)
            } else {
                (self.background, BACKGROUND)
            };
            SetTextColor(
                dc,
                if self.high_contrast {
                    GetSysColor(COLOR_WINDOWTEXT)
                } else if edit
                    || SendMessageW(hwnd, WM_GETFONT, 0, 0) == self.title as isize
                    || SendMessageW(hwnd, WM_GETFONT, 0, 0) == self.heading as isize
                {
                    INK
                } else {
                    MUTED
                },
            );
            SetBkColor(dc, color);
            brush as LRESULT
        }
    }
    pub fn paint(&self, hwnd: HWND, fields: &[HWND], expanded: bool) {
        unsafe {
            let mut paint: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut paint);
            let mut client = RECT::default();
            GetClientRect(hwnd, &mut client);
            // Paint to a backing bitmap to avoid flicker on status/focus updates.
            let memory = CreateCompatibleDC(dc);
            let bitmap = CreateCompatibleBitmap(dc, client.right, client.bottom);
            let old = SelectObject(memory, bitmap);
            FillRect(memory, &client, self.background);
            for (x, y, width, height) in [
                (24, 100, 348, 348),
                (388, 100, 348, 348),
                (24, 468, 712, 152),
            ] {
                self.rounded(
                    memory,
                    RECT {
                        left: self.px(x),
                        top: self.px(y),
                        right: self.px(x + width),
                        bottom: self.px(y + height),
                    },
                    SURFACE,
                    SURFACE,
                    18,
                );
            }
            if expanded {
                self.rounded(
                    memory,
                    RECT {
                        left: self.px(24),
                        top: self.px(680),
                        right: self.px(736),
                        bottom: self.px(800),
                    },
                    SURFACE,
                    SURFACE,
                    18,
                );
            }
            for &field in fields {
                if GetWindowLongW(field, GWL_STYLE) as u32 & WS_VISIBLE == 0 {
                    continue;
                }
                let mut rect = RECT::default();
                GetWindowRect(field, &mut rect);
                MapWindowPoints(null_mut(), hwnd, &mut rect as *mut _ as *mut POINT, 2);
                InflateRect(&mut rect, self.px(10), self.px(9));
                self.rounded(
                    memory,
                    rect,
                    FIELD,
                    if GetFocus() == field { FOCUS } else { OUTLINE },
                    10,
                );
            }
            BitBlt(dc, 0, 0, client.right, client.bottom, memory, 0, 0, SRCCOPY);
            SelectObject(memory, old);
            DeleteObject(bitmap);
            DeleteDC(memory);
            EndPaint(hwnd, &paint);
        }
    }
    fn rounded(&self, dc: HDC, rect: RECT, fill: COLORREF, stroke: COLORREF, radius: i32) {
        unsafe {
            let fill = if self.high_contrast {
                GetSysColor(COLOR_WINDOW)
            } else {
                fill
            };
            let stroke = if self.high_contrast {
                GetSysColor(COLOR_WINDOWTEXT)
            } else {
                stroke
            };
            let brush = CreateSolidBrush(fill);
            let pen = CreatePen(PS_SOLID, self.px(1).max(1), stroke);
            let old_brush = SelectObject(dc, brush);
            let old_pen = SelectObject(dc, pen);
            RoundRect(
                dc,
                rect.left,
                rect.top,
                rect.right,
                rect.bottom,
                self.px(radius),
                self.px(radius),
            );
            SelectObject(dc, old_brush);
            SelectObject(dc, old_pen);
            DeleteObject(brush);
            DeleteObject(pen);
        }
    }
    pub fn draw_button(&self, draw: &DRAWITEMSTRUCT) {
        unsafe {
            let primary = draw.CtlID == super::START as u32;
            let tonal = draw.CtlID == super::COPY as u32;
            let disabled = draw.itemState & ODS_DISABLED != 0;
            let pressed = draw.itemState & ODS_SELECTED != 0;
            let mut cursor = POINT::default();
            GetCursorPos(&mut cursor);
            let mut screen = RECT::default();
            GetWindowRect(draw.hwndItem, &mut screen);
            let hover = PtInRect(&screen, cursor) != 0;
            let fill = if disabled {
                rgb(43, 43, 43)
            } else if primary {
                if pressed {
                    PRIMARY_PRESSED
                } else if hover {
                    PRIMARY_HOVER
                } else {
                    PRIMARY_FILL
                }
            } else if pressed {
                rgb(60, 60, 60)
            } else if hover || tonal {
                rgb(43, 43, 43)
            } else {
                SURFACE
            };
            let ink = if disabled {
                rgb(134, 134, 134)
            } else if primary {
                ON_PRIMARY
            } else {
                INK
            };
            FillRect(
                draw.hDC,
                &draw.rcItem,
                if draw.CtlID == super::EXIT as u32 || draw.CtlID == super::DIAGNOSTICS as u32 {
                    self.background
                } else {
                    self.surface
                },
            );
            self.rounded(
                draw.hDC,
                draw.rcItem,
                fill,
                if primary || tonal { fill } else { OUTLINE },
                12,
            );
            let old_font = SelectObject(draw.hDC, self.body);
            SetBkMode(draw.hDC, TRANSPARENT as i32);
            SetTextColor(draw.hDC, ink);
            let text = super::wide(&super::App::text(draw.hwndItem));
            let mut rect = draw.rcItem;
            DrawTextW(
                draw.hDC,
                text.as_ptr(),
                -1,
                &mut rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
            if draw.itemState & ODS_FOCUS != 0 && draw.itemState & ODS_NOFOCUSRECT == 0 {
                InflateRect(&mut rect, -self.px(4), -self.px(4));
                DrawFocusRect(draw.hDC, &rect);
            }
            SelectObject(draw.hDC, old_font);
        }
    }
}
impl Drop for Style {
    fn drop(&mut self) {
        unsafe {
            for object in [
                self.body,
                self.small,
                self.heading,
                self.title,
                self.background,
                self.surface,
                self.field,
            ] {
                DeleteObject(object);
            }
        }
    }
}
// Cursor feedback is independent of custom painting, including high contrast.
unsafe extern "system" fn pointer_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    unsafe {
        if message == WM_SETCURSOR && (lparam as u32 & 0xffff) == HTCLIENT {
            let cursor = if IsWindowEnabled(hwnd) != 0 {
                IDC_HAND
            } else {
                IDC_ARROW
            };
            SetCursor(LoadCursorW(null_mut(), cursor));
            return 1;
        }
        if message == WM_NCDESTROY {
            RemoveWindowSubclass(hwnd, Some(pointer_proc), 3);
        }
        DefSubclassProc(hwnd, message, wparam, lparam)
    }
}

unsafe extern "system" fn button_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    unsafe {
        if message == WM_MOUSEMOVE {
            let mut tracking = TRACKMOUSEEVENT {
                cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            TrackMouseEvent(&mut tracking);
            InvalidateRect(hwnd, std::ptr::null(), 0);
        } else if message == WM_MOUSELEAVE {
            InvalidateRect(hwnd, std::ptr::null(), 0);
        } else if message == WM_NCDESTROY {
            RemoveWindowSubclass(hwnd, Some(button_proc), 1);
        }
        DefSubclassProc(hwnd, message, wparam, lparam)
    }
}

unsafe extern "system" fn checkbox_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    data: usize,
) -> LRESULT {
    unsafe {
        let style = &*(data as *const Style);
        if message == WM_PAINT {
            let mut paint: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut paint);
            style.draw_checkbox(hwnd, dc);
            EndPaint(hwnd, &paint);
            return 0;
        }
        if message == WM_PRINTCLIENT {
            style.draw_checkbox(hwnd, wparam as HDC);
            return 0;
        }
        if message == WM_ERASEBKGND {
            return 1;
        }
        if message == WM_NCDESTROY {
            RemoveWindowSubclass(hwnd, Some(checkbox_proc), 2);
        }
        let result = DefSubclassProc(hwnd, message, wparam, lparam);
        if matches!(
            message,
            BM_SETCHECK | WM_SETFOCUS | WM_KILLFOCUS | WM_ENABLE | WM_KEYUP | WM_LBUTTONUP
        ) {
            InvalidateRect(hwnd, std::ptr::null(), 0);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clickable_controls_use_hand_cursor_but_disabled_and_text_controls_do_not() {
        struct Window(HWND);
        impl Drop for Window {
            fn drop(&mut self) {
                unsafe { DestroyWindow(self.0) };
            }
        }
        struct CursorRestore(HCURSOR);
        impl Drop for CursorRestore {
            fn drop(&mut self) {
                unsafe { SetCursor(self.0) };
            }
        }
        unsafe {
            let _restore = CursorRestore(GetCursor());
            let style = Style::new();
            let create = |class: &str, flags| {
                let window = Window(CreateWindowExW(
                    0,
                    super::super::wide(class).as_ptr(),
                    super::super::wide("isolated-cursor-test").as_ptr(),
                    WS_POPUP | flags,
                    0,
                    0,
                    160,
                    32,
                    null_mut(),
                    null_mut(),
                    null_mut(),
                    std::ptr::null(),
                ));
                assert!(!window.0.is_null());
                window
            };
            let button = create("BUTTON", BS_PUSHBUTTON as u32);
            let checkbox = create("BUTTON", BS_AUTOCHECKBOX as u32);
            let edit = create("EDIT", ES_AUTOHSCROLL as u32);
            style.prepare_button(button.0);
            style.prepare_checkbox(checkbox.0);
            let hover = |hwnd| {
                SendMessageW(
                    hwnd,
                    WM_SETCURSOR,
                    hwnd as usize,
                    ((WM_MOUSEMOVE as isize) << 16) | HTCLIENT as isize,
                );
                GetCursor()
            };
            assert_eq!(hover(button.0), LoadCursorW(null_mut(), IDC_HAND));
            assert_eq!(hover(checkbox.0), LoadCursorW(null_mut(), IDC_HAND));
            EnableWindow(button.0, 0);
            assert_eq!(hover(button.0), LoadCursorW(null_mut(), IDC_ARROW));
            EnableWindow(checkbox.0, 0);
            assert_eq!(hover(checkbox.0), LoadCursorW(null_mut(), IDC_ARROW));
            assert_eq!(hover(edit.0), LoadCursorW(null_mut(), IDC_IBEAM));
        }
    }
    fn luminance(color: COLORREF) -> f64 {
        let linear = |component: u32| {
            let value = component as f64 / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(color & 255)
            + 0.7152 * linear((color >> 8) & 255)
            + 0.0722 * linear((color >> 16) & 255)
    }
    #[test]
    fn dark_theme_text_has_readable_contrast() {
        for (text, surface) in [
            (INK, SURFACE),
            (INK, FIELD),
            (MUTED, SURFACE),
            (MUTED, BACKGROUND),
            (ON_PRIMARY, PRIMARY_FILL),
            (ON_PRIMARY, PRIMARY_HOVER),
            (ON_PRIMARY, PRIMARY_PRESSED),
        ] {
            let a = luminance(text);
            let b = luminance(surface);
            assert!((a.max(b) + 0.05) / (a.min(b) + 0.05) >= 4.5);
        }
    }
    #[test]
    fn modern_dark_surfaces_are_neutral_grays() {
        for color in [
            BACKGROUND,
            SURFACE,
            FIELD,
            INK,
            MUTED,
            OUTLINE,
            PRIMARY_FILL,
            PRIMARY_HOVER,
            PRIMARY_PRESSED,
            ON_PRIMARY,
            FOCUS,
        ] {
            assert_eq!(color & 255, (color >> 8) & 255);
            assert_eq!(color & 255, (color >> 16) & 255);
        }
    }
}
