#include "image.hpp"
#include <iostream>
#include <dwmapi.h>

static LRESULT CALLBACK paint_fixture(HWND hwnd, UINT message, WPARAM wparam, LPARAM lparam) {
    auto brush = reinterpret_cast<HBRUSH>(GetClassLongPtrW(hwnd, GCLP_HBRBACKGROUND));
    if (message == WM_PAINT) {
        PAINTSTRUCT paint{};
        HDC dc = BeginPaint(hwnd, &paint);
        FillRect(dc, &paint.rcPaint, brush);
        EndPaint(hwnd, &paint);
        return 0;
    }
    if (message == WM_PRINT || message == WM_PRINTCLIENT) {
        RECT rect{};
        GetClientRect(hwnd, &rect);
        FillRect(reinterpret_cast<HDC>(wparam), &rect, brush);
        return 0;
    }
    return DefWindowProcW(hwnd, message, wparam, lparam);
}

// Build as BrokenArrow.exe so discovery exercises the production process filter.
int wmain() {
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    WNDCLASSW wc{};
    wc.hInstance = GetModuleHandleW(nullptr);
    wc.lpfnWndProc = paint_fixture;
    wc.lpszClassName = L"OcrTestWindow";
    wc.hbrBackground = CreateSolidBrush(RGB(40, 80, 120));
    if (!RegisterClassW(&wc)) return 2;
    auto game = CreateWindowExW(WS_EX_NOACTIVATE, wc.lpszClassName, L"", WS_POPUP,
                                0, 0, 1280, 720, nullptr, nullptr, wc.hInstance, nullptr);
    if (!game) return 3;
    SetWindowPos(game, HWND_BOTTOM, 0, 0, 0, 0, SWP_SHOWWINDOW | SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOMOVE);
    InvalidateRect(game, nullptr, TRUE);
    UpdateWindow(game);
    DwmFlush();
    auto foreground = GetForegroundWindow();
    HWND found = nullptr;
    if (find_game_window(game, reinterpret_cast<LPARAM>(&found)) || found != game) return 7;
    // DWM can initially return a black frame for a newly created surface.
    bool matched = false;
    for (int attempt = 0; attempt < 30 && !matched; ++attempt) {
        DwmFlush();
        auto frame = capture_window(found);
        if (foreground == game || GetForegroundWindow() == game ||
            frame.width != 1280 || frame.height != 720) return 4;
        size_t center = (360 * 1280 + 640) * 3;
        matched = frame.pixels[center] == 120 && frame.pixels[center + 1] == 80 &&
                  frame.pixels[center + 2] == 40;
        if (!matched) Sleep(30);
    }
    if (!matched) return 5;
    ShowWindow(game, SW_SHOWMINNOACTIVE);
    if (is_game_window(game) || !capture_window(game).empty()) return 6;
    DestroyWindow(game);
    std::cout << "background capture and minimized skip passed\n";
    return 0;
}
