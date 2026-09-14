#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
#include "wingui.h"

// Global drawing buffer
typedef struct {
    HWND hwnd;
    HDC hdc;
    HDC memDC;
    HBITMAP memBitmap;
    int width;
    int height;
} WindowContext;

// Window procedure
LRESULT CALLBACK WndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    WindowContext* ctx = (WindowContext*)GetWindowLongPtr(hwnd, GWLP_USERDATA);
    
    switch(msg) {
        case WM_PAINT: {
            if (ctx && ctx->memDC) {
                PAINTSTRUCT ps;
                HDC hdc = BeginPaint(hwnd, &ps);
                BitBlt(hdc, 0, 0, ctx->width, ctx->height, ctx->memDC, 0, 0, SRCCOPY);
                EndPaint(hwnd, &ps);
            }
            return 0;
        }
        case WM_CLOSE:
            DestroyWindow(hwnd);
            return 0;
        case WM_DESTROY:
            PostQuitMessage(0);
            return 0;
        default:
            return DefWindowProc(hwnd, msg, wParam, lParam);
    }
}

// Create a window
void* create_window(const char* title, int32_t width, int32_t height) {
    const char CLASS_NAME[] = "AdeshWindowClass";
    
    WNDCLASS wc = {0};
    wc.lpfnWndProc = WndProc;
    wc.hInstance = GetModuleHandle(NULL);
    wc.lpszClassName = CLASS_NAME;
    wc.hCursor = LoadCursor(NULL, IDC_ARROW);
    wc.hbrBackground = (HBRUSH)(COLOR_WINDOW+1);
    
    RegisterClass(&wc);
    
    HWND hwnd = CreateWindowEx(
        0,
        CLASS_NAME,
        title,
        WS_OVERLAPPEDWINDOW,
        CW_USEDEFAULT, CW_USEDEFAULT, width, height,
        NULL,
        NULL,
        GetModuleHandle(NULL),
        NULL
    );
    
    if (hwnd == NULL) {
        return NULL;
    }
    
    // Create context
    WindowContext* ctx = (WindowContext*)malloc(sizeof(WindowContext));
    ctx->hwnd = hwnd;
    ctx->width = width;
    ctx->height = height;
    
    // Create memory DC for double buffering
    ctx->hdc = GetDC(hwnd);
    ctx->memDC = CreateCompatibleDC(ctx->hdc);
    ctx->memBitmap = CreateCompatibleBitmap(ctx->hdc, width, height);
    SelectObject(ctx->memDC, ctx->memBitmap);
    
    // Clear to white
    RECT rect = {0, 0, width, height};
    HBRUSH brush = CreateSolidBrush(RGB(255, 255, 255));
    FillRect(ctx->memDC, &rect, brush);
    DeleteObject(brush);
    
    SetWindowLongPtr(hwnd, GWLP_USERDATA, (LONG_PTR)ctx);
    
    return ctx;
}

// Show window
void show_window(void* handle) {
    if (!handle) return;
    WindowContext* ctx = (WindowContext*)handle;
    ShowWindow(ctx->hwnd, SW_SHOW);
    UpdateWindow(ctx->hwnd);
}

// Process messages (returns 0 when WM_QUIT received)
int32_t process_messages(void* handle) {
    if (!handle) return 0;
    WindowContext* ctx = (WindowContext*)handle;
    
    MSG msg;
    // Process all available messages without blocking
    while (PeekMessage(&msg, ctx->hwnd, 0, 0, PM_REMOVE)) {
        if (msg.message == WM_QUIT) {
            return 0;
        }
        TranslateMessage(&msg);
        DispatchMessage(&msg);
    }
    
    // Sleep a bit to avoid CPU spinning
    Sleep(10);
    return 1;
}

// Run blocking message loop (stays until window is closed)
void run_message_loop(void* handle) {
    if (!handle) return;
    WindowContext* ctx = (WindowContext*)handle;
    
    MSG msg;
    while (GetMessage(&msg, NULL, 0, 0) > 0) {
        TranslateMessage(&msg);
        DispatchMessage(&msg);
    }
}

// Close window
void close_window(void* handle) {
    if (!handle) return;
    WindowContext* ctx = (WindowContext*)handle;
    
    if (ctx->memBitmap) DeleteObject(ctx->memBitmap);
    if (ctx->memDC) DeleteDC(ctx->memDC);
    if (ctx->hdc) ReleaseDC(ctx->hwnd, ctx->hdc);
    if (ctx->hwnd) DestroyWindow(ctx->hwnd);
    
    free(ctx);
}

// Clear window with color
void clear_window(void* handle, uint32_t color) {
    if (!handle) return;
    WindowContext* ctx = (WindowContext*)handle;
    
    RECT rect = {0, 0, ctx->width, ctx->height};
    HBRUSH brush = CreateSolidBrush(RGB(
        (color >> 16) & 0xFF,
        (color >> 8) & 0xFF,
        color & 0xFF
    ));
    FillRect(ctx->memDC, &rect, brush);
    DeleteObject(brush);
}

// Draw circle
void draw_circle(void* handle, int32_t x, int32_t y, int32_t radius, uint32_t color) {
    if (!handle) return;
    WindowContext* ctx = (WindowContext*)handle;
    
    HPEN pen = CreatePen(PS_SOLID, 1, RGB(
        (color >> 16) & 0xFF,
        (color >> 8) & 0xFF,
        color & 0xFF
    ));
    HBRUSH brush = CreateSolidBrush(RGB(
        (color >> 16) & 0xFF,
        (color >> 8) & 0xFF,
        color & 0xFF
    ));
    
    SelectObject(ctx->memDC, pen);
    SelectObject(ctx->memDC, brush);
    
    Ellipse(ctx->memDC, x - radius, y - radius, x + radius, y + radius);
    
    DeleteObject(pen);
    DeleteObject(brush);
}

// Draw rectangle
void draw_rectangle(void* handle, int32_t x, int32_t y, int32_t width, int32_t height, uint32_t color) {
    if (!handle) return;
    WindowContext* ctx = (WindowContext*)handle;
    
    HBRUSH brush = CreateSolidBrush(RGB(
        (color >> 16) & 0xFF,
        (color >> 8) & 0xFF,
        color & 0xFF
    ));
    
    RECT rect = {x, y, x + width, y + height};
    FillRect(ctx->memDC, &rect, brush);
    DeleteObject(brush);
}

// Draw line
void draw_line(void* handle, int32_t x1, int32_t y1, int32_t x2, int32_t y2, uint32_t color) {
    if (!handle) return;
    WindowContext* ctx = (WindowContext*)handle;
    
    HPEN pen = CreatePen(PS_SOLID, 2, RGB(
        (color >> 16) & 0xFF,
        (color >> 8) & 0xFF,
        color & 0xFF
    ));
    
    HPEN oldPen = SelectObject(ctx->memDC, pen);
    MoveToEx(ctx->memDC, x1, y1, NULL);
    LineTo(ctx->memDC, x2, y2);
    SelectObject(ctx->memDC, oldPen);
    
    DeleteObject(pen);
}

// Draw ellipse
void draw_ellipse(void* handle, int32_t x, int32_t y, int32_t width, int32_t height, uint32_t color) {
    if (!handle) return;
    WindowContext* ctx = (WindowContext*)handle;
    
    HPEN pen = CreatePen(PS_SOLID, 1, RGB(
        (color >> 16) & 0xFF,
        (color >> 8) & 0xFF,
        color & 0xFF
    ));
    HBRUSH brush = CreateSolidBrush(RGB(
        (color >> 16) & 0xFF,
        (color >> 8) & 0xFF,
        color & 0xFF
    ));
    
    SelectObject(ctx->memDC, pen);
    SelectObject(ctx->memDC, brush);
    
    Ellipse(ctx->memDC, x, y, x + width, y + height);
    
    DeleteObject(pen);
    DeleteObject(brush);
}

// Draw pixel
void draw_pixel(void* handle, int32_t x, int32_t y, uint32_t color) {
    if (!handle) return;
    WindowContext* ctx = (WindowContext*)handle;
    
    SetPixel(ctx->memDC, x, y, RGB(
        (color >> 16) & 0xFF,
        (color >> 8) & 0xFF,
        color & 0xFF
    ));
}

// Refresh window (redraw)
void refresh_window(void* handle) {
    if (!handle) return;
    WindowContext* ctx = (WindowContext*)handle;
    InvalidateRect(ctx->hwnd, NULL, FALSE);
    UpdateWindow(ctx->hwnd);
}

// Set window title
void set_window_title(void* handle, const char* title) {
    if (!handle) return;
    WindowContext* ctx = (WindowContext*)handle;
    SetWindowText(ctx->hwnd, title);
}
